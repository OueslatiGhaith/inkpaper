use crate::{
    AlignItems, Display, Edges, FlexBasis, FlexDirection, FrameArena, JustifyContent, Length,
    NodeId, NodeKind, Offset, Pixels, Point, Rect, ResolvedTextStyle, Size, Style, px,
};

pub trait TextMeasurer {
    fn measure_text(&self, text: &str, style: ResolvedTextStyle, max_size: Size) -> Size;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Axis {
    Horizontal,
    Vertical,
}

fn main_size(size: Size, axis: Axis) -> Pixels {
    match axis {
        Axis::Horizontal => size.width,
        Axis::Vertical => size.height,
    }
}

fn cross_size(size: Size, axis: Axis) -> Pixels {
    match axis {
        Axis::Horizontal => size.height,
        Axis::Vertical => size.width,
    }
}

fn size_from_axes(main: Pixels, cross: Pixels, axis: Axis) -> Size {
    match axis {
        Axis::Horizontal => Size::new(main, cross),
        Axis::Vertical => Size::new(cross, main),
    }
}

fn point_from_axes(main: Pixels, cross: Pixels, axis: Axis) -> Point {
    match axis {
        Axis::Horizontal => Point::new(main, cross),
        Axis::Vertical => Point::new(cross, main),
    }
}

fn requested_length(style: Style, axis: Axis) -> Length {
    match axis {
        Axis::Horizontal => style.width,
        Axis::Vertical => style.height,
    }
}

fn flow_axis(style: Style) -> Axis {
    match style.display {
        Display::Block => Axis::Vertical,
        Display::Flex => match style.flex_direction {
            FlexDirection::Row => Axis::Horizontal,
            FlexDirection::Column => Axis::Vertical,
        },
    }
}

fn content_available(style: Style, outer_available: Size) -> Size {
    Size::new(
        (outer_available.width - horizontal_chrome(style)).non_negative(),
        (outer_available.height - vertical_chrome(style)).non_negative(),
    )
}

fn border_width(style: Style) -> Pixels {
    style.border_width.non_negative()
}

fn horizontal_chrome(style: Style) -> Pixels {
    style.padding.left.non_negative() + style.padding.right.non_negative() + border_width(style) * 2
}

fn vertical_chrome(style: Style) -> Pixels {
    style.padding.top.non_negative() + style.padding.bottom.non_negative() + border_width(style) * 2
}

fn resolve_dimension(
    length: Length,
    minimum: Option<Pixels>,
    maximum: Option<Pixels>,
    available: Pixels,
    natural: Pixels,
) -> Pixels {
    let available = available.non_negative();
    let base = match length {
        Length::Auto => natural.non_negative().min(available),
        Length::Pixels(value) => value.non_negative(),
        Length::Fill => available,
    };

    let minimum = minimum.map(Pixels::non_negative).unwrap_or(px(0));
    let maximum = maximum
        .map(Pixels::non_negative)
        .unwrap_or(Pixels::MAX)
        .max(minimum);

    base.clamp(minimum, maximum)
}

fn measurement_limit(length: Length, maximum: Option<Pixels>, available: Pixels) -> Pixels {
    let available = available.non_negative();
    let requested = match length {
        Length::Auto => available,
        Length::Pixels(value) => value.non_negative(),
        Length::Fill => available,
    };

    let maximum = maximum.map(Pixels::non_negative).unwrap_or(Pixels::MAX);

    requested.min(maximum)
}

fn main_margin_start(margin: Edges<Pixels>, axis: Axis) -> Pixels {
    match axis {
        Axis::Horizontal => margin.left.non_negative(),
        Axis::Vertical => margin.top.non_negative(),
    }
}

fn main_margin_end(margin: Edges<Pixels>, axis: Axis) -> Pixels {
    match axis {
        Axis::Horizontal => margin.right.non_negative(),
        Axis::Vertical => margin.bottom.non_negative(),
    }
}

fn cross_margin_start(margin: Edges<Pixels>, axis: Axis) -> Pixels {
    match axis {
        Axis::Horizontal => margin.top.non_negative(),
        Axis::Vertical => margin.left.non_negative(),
    }
}

fn cross_margin_end(margin: Edges<Pixels>, axis: Axis) -> Pixels {
    match axis {
        Axis::Horizontal => margin.bottom.non_negative(),
        Axis::Vertical => margin.right.non_negative(),
    }
}

fn main_margin_total(margin: Edges<Pixels>, axis: Axis) -> Pixels {
    main_margin_start(margin, axis).saturating_add(main_margin_end(margin, axis))
}

fn cross_margin_total(margin: Edges<Pixels>, axis: Axis) -> Pixels {
    cross_margin_start(margin, axis).saturating_add(cross_margin_end(margin, axis))
}

fn child_constraint(
    content_size: Size,
    margin: Edges<Pixels>,
    axis: Axis,
    allocated_main: Option<Pixels>,
) -> Size {
    let main = allocated_main.unwrap_or_else(|| {
        (main_size(content_size, axis) - main_margin_total(margin, axis)).non_negative()
    });
    let cross = (cross_size(content_size, axis) - cross_margin_total(margin, axis)).non_negative();

    size_from_axes(main, cross, axis)
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

    let amount = amount.get() as u64;
    let previous = amount.saturating_mul(previous_weight) / total_weight;
    let next = amount.saturating_mul(next_weight) / total_weight;
    let share = next.saturating_sub(previous);

    px(i32::try_from(share).unwrap_or(i32::MAX))
}

fn size_with_main(size: Size, main: Pixels, axis: Axis) -> Size {
    match axis {
        Axis::Horizontal => Size::new(main, size.height),
        Axis::Vertical => Size::new(size.width, main),
    }
}

impl<const NODES: usize, const TEXT_BYTES: usize> FrameArena<NODES, TEXT_BYTES> {
    fn child_count(&self, parent: NodeId) -> usize {
        let mut count = 0;
        let mut current = self.node(parent).first_child;

        while let Some(child) = current {
            count += 1;
            current = self.node(child).next_sibling;
        }

        count
    }

    fn measure_node(
        &self,
        node: NodeId,
        available: Size,
        text_measurer: &dyn TextMeasurer,
    ) -> Size {
        match self.node(node).kind {
            crate::NodeKind::Text { text } => {
                let measured = text_measurer.measure_text(
                    self.text(text),
                    self.node(node).effective_text_style,
                    available,
                );

                Size::new(
                    measured
                        .width
                        .non_negative()
                        .min(available.width.non_negative()),
                    measured
                        .height
                        .non_negative()
                        .min(available.height.non_negative()),
                )
            }
            crate::NodeKind::Entity { .. } => match self.node(node).first_child {
                Some(child) => self.measure_node(child, available, text_measurer),
                None => Size::ZERO,
            },
            crate::NodeKind::Div { .. } => {
                let style = self.node(node).style().expect("div node must have style");
                self.measure_div(node, style, available, text_measurer)
            }
        }
    }

    fn measure_div(
        &self,
        node: NodeId,
        style: Style,
        available: Size,
        text_measurer: &dyn TextMeasurer,
    ) -> Size {
        let axis = flow_axis(style);

        let outer_limit = Size::new(
            measurement_limit(style.width, style.max_width, available.width),
            measurement_limit(style.height, style.max_height, available.height),
        );

        let viewport = content_available(style, outer_limit);
        let child_available = self.child_layout_available(node, viewport);
        let child_count = self.child_count(node);
        let gap = style.gap.non_negative();

        let total_gap = if child_count > 1 {
            let gap_count = i32::try_from(child_count - 1).unwrap_or(i32::MAX);
            gap * gap_count
        } else {
            px(0)
        };

        let mut natural_main = total_gap;
        let mut natural_cross = px(0);
        let mut current = self.node(node).first_child;

        while let Some(child) = current {
            let margin = self.node_margin(child);
            let base = self.flex_base_main_size(child, axis, child_available, text_measurer);
            let measured = self.measure_node(
                child,
                child_constraint(child_available, margin, axis, Some(base)),
                text_measurer,
            );

            natural_main += base + main_margin_total(margin, axis);
            natural_cross =
                natural_cross.max(cross_size(measured, axis) + cross_margin_total(margin, axis));

            current = self.node(child).next_sibling;
        }

        let natural_content = size_from_axes(natural_main, natural_cross, axis);
        let natural_width = natural_content.width + horizontal_chrome(style);
        let natural_height = natural_content.height + vertical_chrome(style);

        Size::new(
            resolve_dimension(
                style.width,
                style.min_width,
                style.max_width,
                available.width,
                natural_width,
            ),
            resolve_dimension(
                style.height,
                style.min_height,
                style.max_height,
                available.height,
                natural_height,
            ),
        )
    }

    fn layout_node(
        &mut self,
        node: NodeId,
        origin: Point,
        available: Size,
        text_measurer: &dyn TextMeasurer,
    ) -> Size {
        let measured = self.measure_node(node, available, text_measurer);
        self.layout_node_with_size(node, origin, measured, text_measurer);

        measured
    }

    fn layout_div_children(
        &mut self,
        node: NodeId,
        style: Style,
        origin: Point,
        outer_size: Size,
        text_measurer: &dyn TextMeasurer,
    ) {
        let child_count = self.child_count(node);
        if child_count == 0 {
            return;
        }

        let axis = flow_axis(style);
        let border = border_width(style);
        let content_origin = Point::new(
            origin.x + border + style.padding.left.non_negative(),
            origin.y + border + style.padding.top.non_negative(),
        );

        let viewport_content_size = content_available(style, outer_size);
        let child_available = self.child_layout_available(node, viewport_content_size);
        let scroll_axes = self.node(node).interaction.scroll_axes;
        let scrolling_main = match axis {
            Axis::Horizontal => scroll_axes.horizontal(),
            Axis::Vertical => scroll_axes.vertical(),
        };
        let scrolling_cross = match axis {
            Axis::Horizontal => scroll_axes.vertical(),
            Axis::Vertical => scroll_axes.horizontal(),
        };

        let gap = style.gap.non_negative();
        let total_gap = if child_count > 1 {
            let gap_count = i32::try_from(child_count - 1).unwrap_or(i32::MAX);
            gap * gap_count
        } else {
            px(0)
        };

        let available_main = main_size(viewport_content_size, axis);
        let available_cross = cross_size(viewport_content_size, axis);

        let mut occupied_main = total_gap;
        let mut grow_before = 0u64;
        let mut shrink_before = 0u64;
        let mut current = self.node(node).first_child;

        while let Some(child) = current {
            let margin = self.node_margin(child);
            let target_main = self.flex_item_main_size(
                node,
                child,
                axis,
                child_available,
                available_main,
                total_gap,
                grow_before,
                shrink_before,
                scrolling_main,
                text_measurer,
            );

            occupied_main += target_main + main_margin_total(margin, axis);
            grow_before = grow_before.saturating_add(self.node_flex_grow(child, axis) as u64);

            let base = self.flex_base_main_size(child, axis, child_available, text_measurer);
            let shrink_factor = (self.node_flex_shrink(child, axis) as u64)
                .saturating_mul(base.non_negative().get() as u64);

            shrink_before = shrink_before.saturating_add(shrink_factor);
            current = self.node(child).next_sibling;
        }

        let free_main = (available_main - occupied_main).non_negative();

        let (leading_main, between_extra, mut between_remainder) = match style.justify_content {
            JustifyContent::Start => (px(0), px(0), px(0)),
            JustifyContent::Center => (free_main / 2, px(0), px(0)),
            JustifyContent::End => (free_main, px(0), px(0)),
            JustifyContent::Between => {
                if child_count > 1 {
                    let spaces = i32::try_from(child_count - 1).unwrap_or(i32::MAX);
                    let between_extra = free_main / spaces;
                    let between_remainder = free_main - between_extra * spaces;

                    (px(0), between_extra, between_remainder)
                } else {
                    (px(0), px(0), px(0))
                }
            }
        };

        let content_main_origin = match axis {
            Axis::Horizontal => content_origin.x,
            Axis::Vertical => content_origin.y,
        };
        let content_cross_origin = match axis {
            Axis::Horizontal => content_origin.y,
            Axis::Vertical => content_origin.x,
        };

        let mut cursor = content_main_origin + leading_main;
        let mut grow_before = 0;
        let mut shrink_before = 0;
        let mut current = self.node(node).first_child;

        while let Some(child) = current {
            let next = self.node(child).next_sibling;
            let margin = self.node_margin(child);
            let target_main = self.flex_item_main_size(
                node,
                child,
                axis,
                child_available,
                available_main,
                total_gap,
                grow_before,
                shrink_before,
                scrolling_main,
                text_measurer,
            );

            let constraint = child_constraint(child_available, margin, axis, Some(target_main));
            let measured = self.measure_node(child, constraint, text_measurer);
            let child_size = size_with_main(measured, target_main, axis);
            let child_outer_cross = cross_size(child_size, axis) + cross_margin_total(margin, axis);
            let cross_free = (available_cross - child_outer_cross).non_negative();
            let cross_offset = if scrolling_cross {
                px(0)
            } else {
                match style.align_items {
                    AlignItems::Start => px(0),
                    AlignItems::Center => cross_free / 2,
                    AlignItems::End => cross_free,
                }
            };

            let child_main_origin = cursor + main_margin_start(margin, axis);
            let child_cross_origin =
                content_cross_origin + cross_offset + cross_margin_start(margin, axis);
            let child_origin = point_from_axes(child_main_origin, child_cross_origin, axis);

            self.layout_node_with_size(child, child_origin, child_size, text_measurer);

            cursor += main_margin_start(margin, axis) + target_main + main_margin_end(margin, axis);

            if next.is_some() {
                cursor += gap + between_extra;
                if between_remainder.is_positive() {
                    cursor += px(1);
                    between_remainder -= px(1)
                }
            }

            grow_before = grow_before.saturating_add(self.node_flex_grow(child, axis) as u64);

            let base = self.flex_base_main_size(child, axis, child_available, text_measurer);
            let shrink_factor = (self.node_flex_shrink(child, axis) as u64)
                .saturating_mul(base.non_negative().get() as u64);

            shrink_before = shrink_before.saturating_add(shrink_factor);
            // child_index += 1; // TODO: check
            current = next;
        }
    }

    pub fn layout(
        &mut self,
        root: NodeId,
        viewport: Size,
        text_measurer: &dyn TextMeasurer,
    ) -> Size {
        self.resolve_text_styles(root);
        self.layout_node(root, Point::ZERO, viewport, text_measurer)
    }

    fn node_margin(&self, node: NodeId) -> Edges<Pixels> {
        match self.node(node).kind {
            NodeKind::Div { .. } => {
                self.node(node)
                    .style()
                    .expect("div node must have style")
                    .margin
            }
            NodeKind::Text { .. } => Edges::all(px(0)),
            NodeKind::Entity { .. } => match self.node(node).first_child {
                Some(child) => self.node_margin(child),
                None => Edges::all(px(0)),
            },
        }
    }

    fn node_flex_style(&self, node: NodeId) -> Option<Style> {
        match self.node(node).kind {
            NodeKind::Div { .. } => self.node(node).style(),
            NodeKind::Text { .. } => None,
            NodeKind::Entity { .. } => self
                .node(node)
                .first_child
                .and_then(|child| self.node_flex_style(child)),
        }
    }

    fn node_flex_grow(&self, node: NodeId, axis: Axis) -> u16 {
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

    fn node_flex_shrink(&self, node: NodeId, axis: Axis) -> u16 {
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

    fn flex_base_main_size(
        &self,
        node: NodeId,
        axis: Axis,
        available: Size,
        text_measurer: &dyn TextMeasurer,
    ) -> Pixels {
        let style = self.node_flex_style(node);
        let requested = style.map(|style| requested_length(style, axis));
        let basis = style.map(|style| style.flex_basis);
        let base = match basis {
            Some(FlexBasis::Pixels(value)) => value.non_negative(),
            Some(FlexBasis::Auto) | None => {
                if requested == Some(Length::Fill) {
                    px(0)
                } else {
                    let measured = self.measure_node(node, available, text_measurer);
                    main_size(measured, axis)
                }
            }
        };

        self.clamp_flex_main_size(node, axis, base)
    }

    fn total_flex_grow_weight(&self, parent: NodeId, axis: Axis) -> u64 {
        let mut total = 0u64;
        let mut current = self.node(parent).first_child;

        while let Some(child) = current {
            total = total.saturating_add(self.node_flex_grow(child, axis) as u64);
            current = self.node(child).next_sibling;
        }

        total
    }

    fn total_flex_shrink_factor(
        &self,
        parent: NodeId,
        axis: Axis,
        available: Size,
        text_measurer: &dyn TextMeasurer,
    ) -> u64 {
        let mut total = 0u64;
        let mut current = self.node(parent).first_child;

        while let Some(child) = current {
            let weight = self.node_flex_shrink(child, axis) as u64;
            let base = self.flex_base_main_size(child, axis, available, text_measurer);
            total = total.saturating_add(weight.saturating_mul(base.non_negative().get() as u64));
            current = self.node(child).next_sibling;
        }

        total
    }

    fn total_flex_base_size(
        &self,
        parent: NodeId,
        axis: Axis,
        available: Size,
        text_measurer: &dyn TextMeasurer,
    ) -> Pixels {
        let mut total = px(0);
        let mut current = self.node(parent).first_child;

        while let Some(child) = current {
            let margin = self.node_margin(child);
            let base = self.flex_base_main_size(child, axis, available, text_measurer);
            total += base + main_margin_total(margin, axis);
            current = self.node(child).next_sibling;
        }

        total
    }

    #[allow(clippy::too_many_arguments)]
    fn flex_item_main_size(
        &self,
        parent: NodeId,
        child: NodeId,
        axis: Axis,
        layout_available: Size,
        viewport_main: Pixels,
        total_gap: Pixels,
        grow_before: u64,
        shrink_before: u64,
        scrolling_main: bool,
        text_measurer: &dyn TextMeasurer,
    ) -> Pixels {
        let base = self.flex_base_main_size(child, axis, layout_available, text_measurer);
        if scrolling_main {
            return base;
        }

        let total_base =
            total_gap + self.total_flex_base_size(parent, axis, layout_available, text_measurer);

        if total_base < viewport_main {
            let free = viewport_main - total_base;
            let weight = self.node_flex_grow(child, axis) as u64;
            let total_weight = self.total_flex_grow_weight(parent, axis);
            let added = weighted_share(
                free,
                grow_before,
                grow_before.saturating_add(weight),
                total_weight,
            );

            return self.clamp_flex_main_size(child, axis, base + added);
        }

        if total_base > viewport_main {
            let deficit = total_base - viewport_main;
            let shrink_weight = self.node_flex_shrink(child, axis) as u64;
            let shrink_factor = shrink_weight.saturating_mul(base.non_negative().get() as u64);
            let total_factor =
                self.total_flex_shrink_factor(parent, axis, layout_available, text_measurer);
            let removed = weighted_share(
                deficit,
                shrink_before,
                shrink_before.saturating_add(shrink_factor),
                total_factor,
            );

            return self.clamp_flex_main_size(child, axis, base - removed);
        }

        base
    }

    fn layout_node_with_size(
        &mut self,
        node: NodeId,
        origin: Point,
        size: Size,
        text_measurer: &dyn TextMeasurer,
    ) {
        self.node_mut(node).layout.bounds = Rect::new(origin, size);
        match self.node(node).kind {
            NodeKind::Text { .. } => {}
            NodeKind::Entity { .. } => {
                if let Some(child) = self.node(node).first_child {
                    self.layout_node_with_size(child, origin, size, text_measurer);
                }
            }
            NodeKind::Div { .. } => {
                let style = self.node(node).style().expect("div node must have style");
                self.layout_div_children(node, style, origin, size, text_measurer);
            }
        }
    }

    fn child_layout_available(&self, node: NodeId, viewport: Size) -> Size {
        const SCROLL_LAYOUT_LIMIT: Pixels = px(i32::MAX / 4);
        let axes = self.node(node).interaction.scroll_axes;

        Size::new(
            if axes.horizontal() {
                SCROLL_LAYOUT_LIMIT
            } else {
                viewport.width
            },
            if axes.vertical() {
                SCROLL_LAYOUT_LIMIT
            } else {
                viewport.height
            },
        )
    }

    pub(crate) fn max_scroll_offset(&self, node: NodeId) -> Offset {
        let bounds = self.node(node).layout.bounds;
        let style = self.node(node).style();
        let mut right = bounds.right();
        let mut bottom = bounds.bottom();
        let mut current = self.node(node).first_child;

        while let Some(child) = current {
            let child_bounds = self.node(child).layout.bounds;
            let margin = self.node_margin(child);

            right = right.max(child_bounds.right() + margin.right.non_negative());
            bottom = bottom.max(child_bounds.bottom() + margin.bottom.non_negative());
            current = self.node(child).next_sibling;
        }

        if let Some(style) = style {
            right += style.padding.right.non_negative();
            bottom += style.padding.bottom.non_negative();
        }

        Offset::new(
            (right - bounds.right()).non_negative(),
            (bottom - bounds.bottom()).non_negative(),
        )
    }
}

#[cfg(test)]
mod tests {
    use core::cell::Cell;

    use crate::*;

    type TestRuntime = Runtime<4096, 16, 4096, 32, 64, 512, 32>;

    struct TestTextMeasurer {
        character_width: Pixels,
        line_height: Pixels,
    }

    impl TestTextMeasurer {
        fn new(character_width: i32, line_height: i32) -> Self {
            Self {
                character_width: px(character_width),
                line_height: px(line_height),
            }
        }
    }

    impl TextMeasurer for TestTextMeasurer {
        fn measure_text(&self, text: &str, _style: ResolvedTextStyle, max_size: Size) -> Size {
            let character_count = i32::try_from(text.chars().count()).unwrap_or(i32::MAX);
            let desired_width = self.character_width.saturating_mul(character_count);
            let desired_height = if text.is_empty() {
                Pixels::ZERO
            } else {
                self.line_height
            };

            Size::new(
                desired_width
                    .non_negative()
                    .min(max_size.width.non_negative()),
                desired_height
                    .non_negative()
                    .min(max_size.height.non_negative()),
            )
        }
    }

    fn assert_bounds(actual: Rect, x: i32, y: i32, width: i32, height: i32) {
        assert_eq!(
            actual,
            Rect::new(Point::new(px(x), px(y),), Size::new(px(width), px(height),),)
        );
    }

    #[test]
    fn fixed_div_and_padding_layout_children() {
        let measurer = TestTextMeasurer::new(8, 10);
        let mut frame = FrameArena::<16, 128>::default();

        let root = frame
            .mount(
                div()
                    .w(px(100))
                    .h(px(60))
                    .p(px(10))
                    .child(div().w(px(20)).h(px(15))),
            )
            .unwrap();

        frame.layout(root, Size::new(px(320), px(240)), &measurer);

        let child = frame.node(root).first_child.unwrap();

        assert_bounds(frame.bounds(root), 0, 0, 100, 60);
        assert_bounds(frame.bounds(child), 10, 10, 20, 15);
    }

    #[test]
    fn block_children_flow_vertically_with_gap() {
        let measurer = TestTextMeasurer::new(8, 10);
        let mut frame = FrameArena::<16, 128>::default();

        let root = frame
            .mount(
                div()
                    .w(px(100))
                    .h(px(100))
                    .gap(px(4))
                    .child(div().w(px(20)).h(px(10)))
                    .child(div().w(px(30)).h(px(15))),
            )
            .unwrap();

        frame.layout(root, Size::new(px(100), px(100)), &measurer);

        let first = frame.node(root).first_child.unwrap();
        let second = frame.node(first).next_sibling.unwrap();

        assert_bounds(frame.bounds(first), 0, 0, 20, 10);
        assert_bounds(frame.bounds(second), 0, 14, 30, 15);
    }

    #[test]
    fn flex_row_children_flow_horizontally() {
        let measurer = TestTextMeasurer::new(8, 10);
        let mut frame = FrameArena::<16, 128>::default();

        let root = frame
            .mount(
                div()
                    .flex()
                    .w(px(100))
                    .h(px(40))
                    .gap(px(5))
                    .child(div().w(px(20)).h(px(10)))
                    .child(div().w(px(30)).h(px(10))),
            )
            .unwrap();

        frame.layout(root, Size::new(px(100), px(40)), &measurer);

        let first = frame.node(root).first_child.unwrap();
        let second = frame.node(first).next_sibling.unwrap();

        assert_bounds(frame.bounds(first), 0, 0, 20, 10);
        assert_bounds(frame.bounds(second), 25, 0, 30, 10);
    }

    #[test]
    fn fill_children_split_remaining_main_axis_space() {
        let measurer = TestTextMeasurer::new(8, 10);
        let mut frame = FrameArena::<16, 128>::default();

        let root = frame
            .mount(
                div()
                    .flex()
                    .w(px(100))
                    .h(px(30))
                    .gap(px(5))
                    .child(div().w(px(20)).h(px(10)))
                    .child(div().w_full().h(px(10)))
                    .child(div().w_full().h(px(10))),
            )
            .unwrap();

        frame.layout(root, Size::new(px(100), px(30)), &measurer);

        let fixed = frame.node(root).first_child.unwrap();
        let first_fill = frame.node(fixed).next_sibling.unwrap();
        let second_fill = frame.node(first_fill).next_sibling.unwrap();

        assert_bounds(frame.bounds(fixed), 0, 0, 20, 10);
        assert_bounds(frame.bounds(first_fill), 25, 0, 35, 10);
        assert_bounds(frame.bounds(second_fill), 65, 0, 35, 10);
    }

    #[test]
    fn text_uses_text_measurer_for_intrinsic_size() {
        let measurer = TestTextMeasurer::new(8, 10);
        let mut frame = FrameArena::<16, 128>::default();

        let root = frame.mount(div().child("abc")).unwrap();

        frame.layout(root, Size::new(px(100), px(100)), &measurer);

        let text = frame.node(root).first_child.unwrap();

        assert_bounds(frame.bounds(text), 0, 0, 24, 10);
        assert_bounds(frame.bounds(root), 0, 0, 24, 10);
    }

    #[test]
    fn padding_and_gap_are_combined_in_column_layout() {
        let measurer = TestTextMeasurer::new(8, 10);
        let mut frame = FrameArena::<16, 128>::default();

        let root = frame
            .mount(
                div()
                    .p(px(5))
                    .gap(px(3))
                    .child(div().w(px(20)).h(px(10)))
                    .child(div().w(px(30)).h(px(12))),
            )
            .unwrap();

        frame.layout(root, Size::new(px(100), px(100)), &measurer);

        let first = frame.node(root).first_child.unwrap();
        let second = frame.node(first).next_sibling.unwrap();

        assert_bounds(frame.bounds(root), 0, 0, 40, 35);
        assert_bounds(frame.bounds(first), 5, 5, 20, 10);
        assert_bounds(frame.bounds(second), 5, 18, 30, 12);
    }

    struct Child;

    impl Render for Child {
        fn render<'a>(&'a mut self, _cx: &mut Context<'_, Self>) -> impl crate::IntoElement + 'a {
            div().w(px(30)).h(px(12))
        }
    }

    struct App {
        child: Entity<Child>,
    }

    impl App {
        fn new(cx: &mut Context<Self>) -> Self {
            let child = cx.new(|_| Child).unwrap();

            Self { child }
        }
    }

    impl Render for App {
        fn render<'a>(&'a mut self, _cx: &mut Context<'_, Self>) -> impl crate::IntoElement + 'a {
            div().child(self.child)
        }
    }

    #[test]
    fn entity_nodes_are_layout_transparent() {
        type TestRuntime = Runtime<4096, 16, 4096, 16, 32, 256, 32>;

        let measurer = TestTextMeasurer::new(8, 10);
        let mut runtime = TestRuntime::default();

        let app = runtime.create(App::new).unwrap();
        runtime.rebuild(app).unwrap();

        runtime
            .layout(Size::new(px(100), px(100)), &measurer)
            .unwrap();

        let root = runtime.root_node().unwrap();
        let app_div = runtime.frame().node(root).first_child.unwrap();
        let child_entity = runtime.frame().node(app_div).first_child.unwrap();
        let child_div = runtime.frame().node(child_entity).first_child.unwrap();

        assert_bounds(runtime.frame().bounds(child_entity), 0, 0, 30, 12);
        assert_bounds(runtime.frame().bounds(child_div), 0, 0, 30, 12);
    }

    #[test]
    fn auto_root_sizes_to_its_content() {
        let measurer = TestTextMeasurer::new(8, 10);
        let mut frame = FrameArena::<16, 128>::default();

        let root = frame.mount(div().child("hello")).unwrap();
        let size = frame.layout(root, Size::new(px(320), px(240)), &measurer);

        assert_eq!(size, Size::new(px(40), px(10),));
    }

    #[test]
    fn fill_root_consumes_the_viewport() {
        let measurer = TestTextMeasurer::new(8, 10);
        let mut frame = FrameArena::<16, 128>::default();

        let root = frame.mount(div().w_full().h_full()).unwrap();
        let size = frame.layout(root, Size::new(px(320), px(240)), &measurer);

        assert_eq!(size, Size::new(px(320), px(240),));
        assert_bounds(frame.bounds(root), 0, 0, 320, 240);
    }

    #[test]
    fn items_center_centers_children_on_cross_axis() {
        let measurer = TestTextMeasurer::new(8, 10);
        let mut frame = FrameArena::<16, 128>::default();

        let root = frame
            .mount(
                div()
                    .flex()
                    .items_center()
                    .w(px(100))
                    .h(px(60))
                    .child(div().w(px(20)).h(px(20))),
            )
            .unwrap();

        frame.layout(root, Size::new(px(100), px(60)), &measurer);

        let child = frame.node(root).first_child.unwrap();

        assert_bounds(frame.bounds(child), 0, 20, 20, 20);
    }

    #[test]
    fn justify_center_centers_children_on_main_axis() {
        let measurer = TestTextMeasurer::new(8, 10);
        let mut frame = FrameArena::<16, 128>::default();

        let root = frame
            .mount(
                div()
                    .flex()
                    .justify_center()
                    .w(px(100))
                    .h(px(30))
                    .gap(px(10))
                    .child(div().w(px(20)).h(px(10)))
                    .child(div().w(px(20)).h(px(10))),
            )
            .unwrap();

        frame.layout(root, Size::new(px(100), px(30)), &measurer);

        let first = frame.node(root).first_child.unwrap();
        let second = frame.node(first).next_sibling.unwrap();

        assert_bounds(frame.bounds(first), 25, 0, 20, 10);
        assert_bounds(frame.bounds(second), 55, 0, 20, 10);
    }

    #[test]
    fn justify_between_distributes_remaining_space() {
        let measurer = TestTextMeasurer::new(8, 10);
        let mut frame = FrameArena::<16, 128>::default();

        let root = frame
            .mount(
                div()
                    .flex()
                    .justify_between()
                    .w(px(100))
                    .h(px(30))
                    .child(div().w(px(20)).h(px(10)))
                    .child(div().w(px(20)).h(px(10)))
                    .child(div().w(px(20)).h(px(10))),
            )
            .unwrap();

        frame.layout(root, Size::new(px(100), px(30)), &measurer);

        let first = frame.node(root).first_child.unwrap();
        let second = frame.node(first).next_sibling.unwrap();
        let third = frame.node(second).next_sibling.unwrap();

        assert_bounds(frame.bounds(first), 0, 0, 20, 10);
        assert_bounds(frame.bounds(second), 40, 0, 20, 10);
        assert_bounds(frame.bounds(third), 80, 0, 20, 10);
    }

    #[test]
    fn margins_participate_in_flex_flow() {
        let measurer = TestTextMeasurer::new(8, 10);
        let mut frame = FrameArena::<16, 128>::default();

        let root = frame
            .mount(
                div()
                    .flex()
                    .w(px(100))
                    .h(px(40))
                    .child(div().w(px(20)).h(px(10)).ml(px(5)).mr(px(7)))
                    .child(div().w(px(20)).h(px(10))),
            )
            .unwrap();

        frame.layout(root, Size::new(px(100), px(40)), &measurer);

        let first = frame.node(root).first_child.unwrap();
        let second = frame.node(first).next_sibling.unwrap();

        assert_bounds(frame.bounds(first), 5, 0, 20, 10);
        assert_bounds(frame.bounds(second), 32, 0, 20, 10);
    }

    #[test]
    fn min_width_expands_auto_element() {
        let measurer = TestTextMeasurer::new(8, 10);
        let mut frame = FrameArena::<16, 128>::default();

        let root = frame.mount(div().min_w(px(80)).child("Hi")).unwrap();

        frame.layout(root, Size::new(px(200), px(100)), &measurer);

        assert_eq!(frame.bounds(root).width(), px(80));
    }

    #[test]
    fn max_width_limits_auto_element() {
        let measurer = TestTextMeasurer::new(8, 10);
        let mut frame = FrameArena::<16, 128>::default();

        let root = frame
            .mount(
                div()
                    .max_w(px(40))
                    .child("This is much wider than forty pixels"),
            )
            .unwrap();

        frame.layout(root, Size::new(px(200), px(100)), &measurer);

        assert_eq!(frame.bounds(root).width(), px(40));
    }

    #[test]
    fn border_width_reduces_content_area() {
        let measurer = TestTextMeasurer::new(8, 10);
        let mut frame = FrameArena::<16, 128>::default();

        let root = frame
            .mount(
                div()
                    .w(px(100))
                    .h(px(60))
                    .border(px(2))
                    .p(px(4))
                    .child(div().w(px(20)).h(px(10))),
            )
            .unwrap();

        frame.layout(root, Size::new(px(100), px(60)), &measurer);

        let child = frame.node(root).first_child.unwrap();

        assert_bounds(frame.bounds(root), 0, 0, 100, 60);
        assert_bounds(frame.bounds(child), 6, 6, 20, 10);
    }

    #[test]
    fn flex_1_consumes_space_after_fixed_child() {
        let measurer = TestTextMeasurer::new(8, 10);
        let mut frame = FrameArena::<16, 128>::default();

        let root = frame
            .mount(
                div()
                    .flex()
                    .w(px(200))
                    .h(px(40))
                    .child(div().w(px(50)).h(px(20)))
                    .child(div().flex_1().h(px(20))),
            )
            .unwrap();

        frame.layout(root, Size::new(px(200), px(40)), &measurer);

        let sidebar = frame.node(root).first_child.unwrap();
        let content = frame.node(sidebar).next_sibling.unwrap();

        assert_bounds(frame.bounds(sidebar), 0, 0, 50, 20);
        assert_bounds(frame.bounds(content), 50, 0, 150, 20);
    }

    #[test]
    fn flex_grow_distributes_space_by_weight() {
        let measurer = TestTextMeasurer::new(8, 10);
        let mut frame = FrameArena::<16, 128>::default();

        let root = frame
            .mount(
                div()
                    .flex()
                    .w(px(200))
                    .h(px(40))
                    .child(div().flex_basis(px(0)).flex_grow(1).h(px(20)))
                    .child(div().flex_basis(px(0)).flex_grow(2).h(px(20)))
                    .child(div().flex_basis(px(0)).flex_grow(1).h(px(20))),
            )
            .unwrap();

        frame.layout(root, Size::new(px(200), px(40)), &measurer);

        let first = frame.node(root).first_child.unwrap();
        let second = frame.node(first).next_sibling.unwrap();
        let third = frame.node(second).next_sibling.unwrap();

        assert_bounds(frame.bounds(first), 0, 0, 50, 20);
        assert_bounds(frame.bounds(second), 50, 0, 100, 20);
        assert_bounds(frame.bounds(third), 150, 0, 50, 20);
    }

    #[test]
    fn flex_basis_is_used_before_grow_distribution() {
        let measurer = TestTextMeasurer::new(8, 10);
        let mut frame = FrameArena::<16, 128>::default();

        let root = frame
            .mount(
                div()
                    .flex()
                    .w(px(200))
                    .h(px(40))
                    .child(div().flex_basis(px(50)).flex_grow(1).h(px(20)))
                    .child(div().flex_basis(px(100)).flex_grow(1).h(px(20))),
            )
            .unwrap();

        frame.layout(root, Size::new(px(200), px(40)), &measurer);

        let first = frame.node(root).first_child.unwrap();
        let second = frame.node(first).next_sibling.unwrap();

        assert_bounds(frame.bounds(first), 0, 0, 75, 20);
        assert_bounds(frame.bounds(second), 75, 0, 125, 20);
    }

    #[test]
    fn flex_shrink_reduces_items_when_they_overflow() {
        let measurer = TestTextMeasurer::new(8, 10);
        let mut frame = FrameArena::<16, 128>::default();

        let root = frame
            .mount(
                div()
                    .flex()
                    .w(px(100))
                    .h(px(40))
                    .child(div().flex_basis(px(80)).flex_shrink(1).h(px(20)))
                    .child(div().flex_basis(px(80)).flex_shrink(1).h(px(20))),
            )
            .unwrap();

        frame.layout(root, Size::new(px(100), px(40)), &measurer);

        let first = frame.node(root).first_child.unwrap();
        let second = frame.node(first).next_sibling.unwrap();

        assert_bounds(frame.bounds(first), 0, 0, 50, 20);
        assert_bounds(frame.bounds(second), 50, 0, 50, 20);
    }

    #[test]
    fn flex_shrink_uses_weight_and_base_size() {
        let measurer = TestTextMeasurer::new(8, 10);
        let mut frame = FrameArena::<16, 128>::default();

        let root = frame
            .mount(
                div()
                    .flex()
                    .w(px(100))
                    .h(px(40))
                    .child(div().flex_basis(px(80)).flex_shrink(1).h(px(20)))
                    .child(div().flex_basis(px(80)).flex_shrink(3).h(px(20))),
            )
            .unwrap();

        frame.layout(root, Size::new(px(100), px(40)), &measurer);

        let first = frame.node(root).first_child.unwrap();
        let second = frame.node(first).next_sibling.unwrap();

        assert_bounds(frame.bounds(first), 0, 0, 65, 20);
        assert_bounds(frame.bounds(second), 65, 0, 35, 20);
    }

    #[test]
    fn flex_grow_respects_gap() {
        let measurer = TestTextMeasurer::new(8, 10);
        let mut frame = FrameArena::<16, 128>::default();

        let root = frame
            .mount(
                div()
                    .flex()
                    .w(px(100))
                    .h(px(40))
                    .gap(px(10))
                    .child(div().flex_1().h(px(20)))
                    .child(div().flex_1().h(px(20))),
            )
            .unwrap();

        frame.layout(root, Size::new(px(100), px(40)), &measurer);

        let first = frame.node(root).first_child.unwrap();
        let second = frame.node(first).next_sibling.unwrap();

        assert_bounds(frame.bounds(first), 0, 0, 45, 20);
        assert_bounds(frame.bounds(second), 55, 0, 45, 20);
    }

    #[test]
    fn flex_grow_respects_item_margins() {
        let measurer = TestTextMeasurer::new(8, 10);
        let mut frame = FrameArena::<16, 128>::default();

        let root = frame
            .mount(
                div()
                    .flex()
                    .w(px(100))
                    .h(px(40))
                    .child(div().flex_1().mx(px(5)).h(px(20)))
                    .child(div().flex_1().mx(px(5)).h(px(20))),
            )
            .unwrap();

        frame.layout(root, Size::new(px(100), px(40)), &measurer);

        let first = frame.node(root).first_child.unwrap();
        let second = frame.node(first).next_sibling.unwrap();

        assert_bounds(frame.bounds(first), 5, 0, 40, 20);
        assert_bounds(frame.bounds(second), 55, 0, 40, 20);
    }

    #[test]
    fn fill_on_main_axis_remains_compatible_with_equal_flex_grow() {
        let measurer = TestTextMeasurer::new(8, 10);
        let mut frame = FrameArena::<16, 128>::default();

        let root = frame
            .mount(
                div()
                    .flex()
                    .w(px(100))
                    .h(px(40))
                    .child(div().w_full().h(px(20)))
                    .child(div().w_full().h(px(20))),
            )
            .unwrap();

        frame.layout(root, Size::new(px(100), px(40)), &measurer);

        let first = frame.node(root).first_child.unwrap();
        let second = frame.node(first).next_sibling.unwrap();

        assert_bounds(frame.bounds(first), 0, 0, 50, 20);
        assert_bounds(frame.bounds(second), 50, 0, 50, 20);
    }

    #[test]
    fn vertical_scroll_moves_content_without_relayout() {
        struct App;
        impl Render for App {
            fn render<'a>(&'a mut self, _cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
                div().w(px(100)).h(px(60)).child(
                    div()
                        .id("scroll")
                        .w(px(100))
                        .h(px(40))
                        .overflow_y_scroll()
                        .child(div().w(px(100)).h(px(30)).bg(Color::RED))
                        .child(div().w(px(100)).h(px(30)).bg(Color::BLUE)),
                )
            }
        }

        let mut runtime = TestRuntime::default();

        let app = runtime.create(|_| App).unwrap();

        runtime.rebuild(app).unwrap();

        let measurer = TestTextMeasurer::new(8, 10);

        runtime
            .layout(Size::new(px(100), px(60)), &measurer)
            .unwrap();

        let before = runtime.frame().node_count();

        assert!(runtime.scroll_at(Point::new(px(10), px(10),), Offset::new(px(0), px(20),),));

        assert_eq!(runtime.take_invalidation(), Invalidation::Paint);

        assert_eq!(runtime.frame().node_count(), before);
    }

    #[test]
    fn scroll_offset_is_clamped_to_content_extent() {
        struct App;

        impl Render for App {
            fn render<'a>(&'a mut self, _cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
                div()
                    .id("scroll")
                    .w(px(100))
                    .h(px(40))
                    .overflow_y_scroll()
                    .child(div().w(px(100)).h(px(30)))
                    .child(div().w(px(100)).h(px(30)))
            }
        }

        let mut runtime = TestRuntime::default();

        let app = runtime.create(|_| App).unwrap();

        runtime.rebuild(app).unwrap();
        runtime
            .layout(Size::new(px(100), px(40)), &TestTextMeasurer::new(8, 10))
            .unwrap();

        assert!(runtime.scroll_at(Point::new(px(10), px(10),), Offset::new(px(0), px(1_000),),));

        runtime.take_invalidation();

        assert!(!runtime.scroll_at(Point::new(px(10), px(10),), Offset::new(px(0), px(1),),));
    }

    #[test]
    fn scroll_offset_survives_rebuild() {
        struct App;

        impl Render for App {
            fn render<'a>(&'a mut self, _cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
                div()
                    .id("scroll")
                    .w(px(100))
                    .h(px(40))
                    .overflow_y_scroll()
                    .child(div().h(px(80)).w(px(100)).bg(Color::RED))
            }
        }

        let mut runtime = TestRuntime::default();

        let app = runtime.create(|_| App).unwrap();

        runtime.rebuild(app).unwrap();

        let measurer = TestTextMeasurer::new(8, 10);

        runtime
            .layout(Size::new(px(100), px(40)), &measurer)
            .unwrap();

        assert!(runtime.scroll_at(Point::new(px(10), px(10),), Offset::new(px(0), px(20),),));

        runtime.take_invalidation();
        runtime.rebuild(app).unwrap();
        runtime
            .layout(Size::new(px(100), px(40)), &measurer)
            .unwrap();

        assert!(runtime.scroll_at(Point::new(px(10), px(10),), Offset::new(px(0), px(-1),),));
    }

    #[test]
    fn text_measurement_receives_resolved_text_style() {
        struct RecordingMeasurer {
            style: Cell<Option<ResolvedTextStyle>>,
        }

        impl TextMeasurer for RecordingMeasurer {
            fn measure_text(&self, text: &str, style: ResolvedTextStyle, max_size: Size) -> Size {
                self.style.set(Some(style));

                if text.is_empty() {
                    return Size::ZERO;
                }

                Size::new(
                    px(20).min(max_size.width.non_negative()),
                    px(10).min(max_size.height.non_negative()),
                )
            }
        }

        let mut frame = FrameArena::<8, 64>::default();

        let root = frame
            .mount(
                div()
                    .font(FontId::new(3))
                    .text_color(Color::GREEN)
                    .line_height(px(18))
                    .child("Hello"),
            )
            .unwrap();

        let measurer = RecordingMeasurer {
            style: Cell::new(None),
        };

        frame.layout(root, Size::new(px(100), px(100)), &measurer);

        assert_eq!(
            measurer.style.get(),
            Some(ResolvedTextStyle {
                font: FontId::new(3),
                color: Color::GREEN,
                line_height: LineHeight::Pixels(px(18)),
                ..Default::default()
            })
        );
    }
}
