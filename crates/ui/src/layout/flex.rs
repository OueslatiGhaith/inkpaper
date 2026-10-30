use super::*;

use crate::{
    FlexBasis, FrameArena, NodeId, NodeKind, Position, count_metric, storage::FrameStorage,
};

impl<F: FrameStorage> FrameArena<F> {
    pub(super) fn flow_child_count(&self, parent: NodeId) -> usize {
        let mut count = 0;
        let mut current = self.node(parent).first_child;

        while let Some(child) = current {
            if self.node_position(child) != Position::Absolute {
                count += 1;
            }

            current = self.node(child).next_sibling;
        }

        count
    }
    pub(super) fn node_position(&self, node: NodeId) -> Position {
        self.node_flex_style(node)
            .map(|style| style.position)
            .unwrap_or(Position::Static)
    }
    pub(super) fn has_absolute_children(&self, parent: NodeId) -> bool {
        let mut current = self.node(parent).first_child;

        while let Some(child) = current {
            if self.node_position(child) == Position::Absolute {
                return true;
            }

            current = self.node(child).next_sibling;
        }

        false
    }
    pub(super) fn node_margin(&self, node: NodeId) -> Edges<Pixels> {
        match self.node(node).kind {
            NodeKind::Div { .. } => self.style(node).expect("div node must have style").margin,
            NodeKind::Text { .. }
            | NodeKind::Image { .. }
            | NodeKind::Svg { .. }
            | NodeKind::Canvas { .. } => Edges::all(px(0)),
            NodeKind::Entity { .. } => match self.node(node).first_child {
                Some(child) => self.node_margin(child),
                None => Edges::all(px(0)),
            },
        }
    }
    pub(super) fn node_flex_style(&self, node: NodeId) -> Option<Style> {
        match self.node(node).kind {
            NodeKind::Div { .. } => self.style(node),
            NodeKind::Text { .. }
            | NodeKind::Image { .. }
            | NodeKind::Svg { .. }
            | NodeKind::Canvas { .. } => None,
            NodeKind::Entity { .. } => self
                .node(node)
                .first_child
                .and_then(|child| self.node_flex_style(child)),
        }
    }
    pub(super) fn node_flex_grow(&self, node: NodeId, axis: Axis) -> u16 {
        let Some(style) = self.node_flex_style(node) else {
            return 0;
        };
        if style.flex_grow > 0 {
            return style.flex_grow;
        }
        if requested_length(style, axis) == Length::Fill {
            return 1;
        }

        0
    }
    pub(super) fn node_flex_shrink(&self, node: NodeId, axis: Axis) -> u16 {
        let Some(style) = self.node_flex_style(node) else {
            return 0;
        };
        if style.flex_shrink > 0 {
            return style.flex_shrink;
        }
        if requested_length(style, axis) == Length::Fill {
            return 1;
        }

        0
    }
    fn clamp_flex_main_size(&self, node: NodeId, axis: Axis, value: Pixels) -> Pixels {
        let value = value.non_negative();
        let Some(style) = self.node_flex_style(node) else {
            return value;
        };

        let (minimum, maximum) = match axis {
            Axis::Horizontal => (style.min_width, style.max_width),
            Axis::Vertical => (style.min_height, style.max_height),
        };

        clamp_to_limits(value, minimum, maximum)
    }
    pub(super) fn flex_base_main_size(
        &mut self,
        node: NodeId,
        axis: Axis,
        available: AvailableSize,
        text_measurer: &dyn TextMeasurer,
    ) -> Pixels {
        count_metric!(self, flex_base_main_size_calls);
        let style = self.node_flex_style(node);
        let requested = style.map(|style| requested_length(style, axis));
        let basis = style.map(|style| style.flex_basis);
        let base = match basis {
            Some(FlexBasis::Pixels(value)) => value.non_negative(),
            Some(FlexBasis::Auto) | None => match requested {
                Some(Length::Pixels(value)) => value.non_negative(),
                Some(Length::Fill) => px(0),
                Some(Length::Auto) | None => {
                    let measured = self.measure_node(node, available, text_measurer);
                    main_size(measured, axis)
                }
            },
        };

        self.clamp_flex_main_size(node, axis, base)
    }
    /// how `child` takes part in sharing its line's free space or overflow
    fn flex_share(&self, child: NodeId, axis: Axis, base: Pixels, mode: FlexMode) -> FlexShare {
        let style = self.node_flex_style(child);
        let (minimum, maximum) = match (style, axis) {
            (Some(style), Axis::Horizontal) => (style.min_width, style.max_width),
            (Some(style), Axis::Vertical) => (style.min_height, style.max_height),
            (None, _) => (None, None),
        };

        match mode {
            FlexMode::Grow => FlexShare {
                weight: self.node_flex_grow(child, axis) as u64,
                limit: maximum.map(|maximum| (maximum - base).non_negative()),
            },
            // a child gives up overflow in proportion to its size, so small children
            // aren't squeezed to nothing first
            FlexMode::Shrink => FlexShare {
                weight: (self.node_flex_shrink(child, axis) as u64)
                    .saturating_mul(base.non_negative().get() as u64),
                limit: Some((base - minimum.unwrap_or(Pixels::ZERO)).non_negative()),
            },
        }
    }
    fn flex_totals(
        &mut self,
        parent: NodeId,
        axis: Axis,
        available: AvailableSize,
        text_measurer: &dyn TextMeasurer,
    ) -> FlexTotals {
        let mut totals = FlexTotals {
            base_size: px(0),
            grow: FlexWeights::default(),
            shrink: FlexWeights::default(),
        };

        let mut current = self.node(parent).first_child;

        while let Some(child) = current {
            if self.node_position(child) == Position::Absolute {
                current = self.node(child).next_sibling;
                continue;
            }

            count_metric!(self, flex_sibling_visits);

            let margin = self.node_margin(child);
            let base = self.flex_base_main_size(child, axis, available, text_measurer);

            totals.base_size += base + main_margin_total(margin, axis);
            totals
                .grow
                .add(self.flex_share(child, axis, base, FlexMode::Grow));
            totals
                .shrink
                .add(self.flex_share(child, axis, base, FlexMode::Shrink));

            current = self.node(child).next_sibling;
        }

        totals
    }
    /// how the flow children of `parent` share the difference between their base
    /// sizes and `main_space`. `None` when they fit it exactly
    pub(super) fn flex_line(
        &mut self,
        parent: NodeId,
        axis: Axis,
        available: AvailableSize,
        main_space: Pixels,
        text_measurer: &dyn TextMeasurer,
    ) -> Option<FlexLine> {
        let totals = self.flex_totals(parent, axis, available, text_measurer);
        let (mode, amount, weights) = if totals.base_size < main_space {
            (FlexMode::Grow, main_space - totals.base_size, totals.grow)
        } else if totals.base_size > main_space {
            (
                FlexMode::Shrink,
                totals.base_size - main_space,
                totals.shrink,
            )
        } else {
            return None;
        };

        let mut line = FlexLine {
            mode,
            ratio: FlexRatio {
                amount,
                weight: weights.total,
            },
        };

        // most lines share their space without taking any child past its limit
        if !weights
            .tightest
            .is_some_and(|tightest| line.ratio.passes(tightest.amount, tightest.weight))
        {
            return Some(line);
        }

        // each pass freezes the children the last ratio took past their limits at
        // those limits, and shares what's left between the rest. Freezing children
        // only raises the ratio, so the frozen set only grows until the ratio settles
        loop {
            let mut frozen = px(0);
            let mut weight = 0u64;
            let mut current = self.node(parent).first_child;

            while let Some(child) = current {
                if self.node_position(child) != Position::Absolute {
                    let base = self.flex_base_main_size(child, axis, available, text_measurer);
                    let share = self.flex_share(child, axis, base, mode);

                    match share.frozen_at(line.ratio) {
                        Some(limit) => frozen += limit,
                        None => weight = weight.saturating_add(share.weight),
                    }
                }

                current = self.node(child).next_sibling;
            }

            let ratio = FlexRatio {
                amount: (amount - frozen).non_negative(),
                weight,
            };
            if ratio == line.ratio {
                return Some(line);
            }

            line.ratio = ratio;
        }
    }
    /// the main size of `child` in `line`. `before` is the weight of the children
    /// before it that shared the line's ratio, so rounding never loses a pixel
    pub(super) fn flex_item_main_size(
        &self,
        child: NodeId,
        axis: Axis,
        base: Pixels,
        line: Option<FlexLine>,
        before: &mut u64,
    ) -> Pixels {
        count_metric!(self, flex_item_main_size_calls);
        let Some(line) = line else {
            return base;
        };

        let share = self.flex_share(child, axis, base, line.mode);
        let change = match share.frozen_at(line.ratio) {
            Some(limit) => limit,
            None => {
                let after = before.saturating_add(share.weight);
                let change = weighted_share(line.ratio.amount, *before, after, line.ratio.weight);
                *before = after;
                change
            }
        };

        let size = match line.mode {
            FlexMode::Grow => base + change,
            FlexMode::Shrink => base - change,
        };

        self.clamp_flex_main_size(child, axis, size)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FlexMode {
    Grow,
    Shrink,
}

/// a line's free space or overflow, shared between the children that haven't
/// reached their limits
#[derive(Debug, Clone, Copy)]
pub(super) struct FlexLine {
    mode: FlexMode,
    ratio: FlexRatio,
}

/// space per unit of weight, `amount / weight`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FlexRatio {
    amount: Pixels,
    weight: u64,
}

impl FlexRatio {
    /// whether this ratio gives `weight` more than `limit`. Once no weight is left
    /// to share, every flexible child is at its limit
    fn passes(self, limit: Pixels, weight: u64) -> bool {
        if self.weight == 0 {
            return true;
        }

        u128::from(limit.non_negative().get() as u32) * u128::from(self.weight)
            < u128::from(self.amount.non_negative().get() as u32) * u128::from(weight)
    }
}

/// how one child takes part in sharing its line's space
#[derive(Debug, Clone, Copy)]
struct FlexShare {
    /// how much of the space it takes relative to its siblings
    weight: u64,
    /// the most it can grow or shrink before it reaches its max or min size
    limit: Option<Pixels>,
}

impl FlexShare {
    /// the child's limit, if `ratio` would take it past that
    fn frozen_at(self, ratio: FlexRatio) -> Option<Pixels> {
        self.limit
            .filter(|&limit| self.weight > 0 && ratio.passes(limit, self.weight))
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct FlexWeights {
    total: u64,
    /// the child that reaches its limit at the lowest ratio
    tightest: Option<FlexRatio>,
}

impl FlexWeights {
    fn add(&mut self, share: FlexShare) {
        if share.weight == 0 {
            return;
        }

        self.total = self.total.saturating_add(share.weight);
        if let Some(limit) = share.limit {
            let ratio = FlexRatio {
                amount: limit,
                weight: share.weight,
            };
            if self
                .tightest
                .is_none_or(|tightest| tightest.passes(limit, share.weight))
            {
                self.tightest = Some(ratio);
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct FlexTotals {
    base_size: Pixels,
    grow: FlexWeights,
    shrink: FlexWeights,
}

fn weighted_share(
    amount: Pixels,
    previous_weight: u64,
    next_weight: u64,
    total_weight: u64,
) -> Pixels {
    if amount.is_non_positive() || total_weight <= previous_weight {
        return px(0);
    }

    let amount = u128::from(amount.get() as u32);
    let total_weight = u128::from(total_weight);
    let previous = amount * u128::from(previous_weight) / total_weight;
    let next = amount * u128::from(next_weight) / total_weight;
    let share = next.saturating_sub(previous);

    px(i32::try_from(share).unwrap_or(i32::MAX))
}
