use super::*;

use crate::{FlexBasis, FrameArena, NodeId, NodeKind, Position, count_metric};

impl<const NODES: usize, const TEXT_BYTES: usize> FrameArena<NODES, TEXT_BYTES> {
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
            NodeKind::Div { .. } => {
                self.node(node)
                    .style()
                    .expect("div node must have style")
                    .margin
            }
            NodeKind::Text { .. } | NodeKind::Image { .. } | NodeKind::Canvas { .. } => {
                Edges::all(px(0))
            }
            NodeKind::Entity { .. } => match self.node(node).first_child {
                Some(child) => self.node_margin(child),
                None => Edges::all(px(0)),
            },
        }
    }
    pub(super) fn node_flex_style(&self, node: NodeId) -> Option<Style> {
        match self.node(node).kind {
            NodeKind::Div { .. } => self.node(node).style(),
            NodeKind::Text { .. } | NodeKind::Image { .. } | NodeKind::Canvas { .. } => None,
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

        let minimum = minimum.map(Pixels::non_negative).unwrap_or(px(0));
        let maximum = maximum
            .map(Pixels::non_negative)
            .unwrap_or(Pixels::MAX)
            .max(minimum);

        value.clamp(minimum, maximum)
    }
    pub(super) fn flex_base_main_size(
        &mut self,
        node: NodeId,
        axis: Axis,
        available: Size,
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
    pub(super) fn flex_totals(
        &mut self,
        parent: NodeId,
        axis: Axis,
        available: Size,
        text_measurer: &dyn TextMeasurer,
    ) -> FlexTotals {
        let mut base_size = px(0);
        let mut grow_weight = 0u64;
        let mut shrink_factor = 0u64;

        let mut current = self.node(parent).first_child;

        while let Some(child) = current {
            if self.node_position(child) == Position::Absolute {
                current = self.node(child).next_sibling;
                continue;
            }

            count_metric!(self, flex_sibling_visits);

            let margin = self.node_margin(child);
            let base = self.flex_base_main_size(child, axis, available, text_measurer);
            let grow = self.node_flex_grow(child, axis) as u64;
            let shrink = self.node_flex_shrink(child, axis) as u64;

            base_size += base + main_margin_total(margin, axis);
            grow_weight = grow_weight.saturating_add(grow);
            shrink_factor = shrink_factor
                .saturating_add(shrink.saturating_mul(base.non_negative().get() as u64));

            current = self.node(child).next_sibling;
        }

        FlexTotals {
            base_size,
            grow_weight,
            shrink_factor,
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn flex_item_main_size(
        &self,
        child: NodeId,
        axis: Axis,
        base: Pixels,
        viewport_main: Pixels,
        total_gap: Pixels,
        grow_before: u64,
        shrink_before: u64,
        totals: Option<FlexTotals>,
    ) -> Pixels {
        count_metric!(self, flex_item_main_size_calls);
        let Some(totals) = totals else {
            return base;
        };

        let total_base = total_gap + totals.base_size;

        if total_base < viewport_main {
            let free = viewport_main - total_base;
            let weight = self.node_flex_grow(child, axis) as u64;
            let added = weighted_share(
                free,
                grow_before,
                grow_before.saturating_add(weight),
                totals.grow_weight,
            );

            return self.clamp_flex_main_size(child, axis, base + added);
        }

        if total_base > viewport_main {
            let deficit = total_base - viewport_main;
            let shrink_weight = self.node_flex_shrink(child, axis) as u64;
            let shrink_factor = shrink_weight.saturating_mul(base.non_negative().get() as u64);
            let removed = weighted_share(
                deficit,
                shrink_before,
                shrink_before.saturating_add(shrink_factor),
                totals.shrink_factor,
            );

            return self.clamp_flex_main_size(child, axis, base - removed);
        }

        base
    }
}
