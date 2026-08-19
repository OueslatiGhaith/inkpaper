use crate::{
    AlignItems, CanvasStyle, Display, Edges, FlexBasis, FlexDirection, FrameArena, ImageSource,
    ImageStyle, JustifyContent, Length, NodeId, NodeKind, Offset, Pixels, Point, Position, Rect,
    ResolvedTextStyle, Size, Style, count_metric, px,
};

pub trait TextMeasurer {
    fn measure_text(&self, text: &str, style: ResolvedTextStyle, max_size: Size) -> Size;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Axis {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy)]
struct FlexTotals {
    base_size: Pixels,
    grow_weight: u64,
    shrink_factor: u64,
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

pub(crate) fn flow_axis(style: Style) -> Axis {
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

fn content_rect(style: Style, origin: Point, outer_size: Size) -> Rect {
    let border = border_width(style);

    Rect::new(
        Point::new(
            origin.x + border + style.padding.left.non_negative(),
            origin.y + border + style.padding.top.non_negative(),
        ),
        content_available(style, outer_size),
    )
}

fn absolute_axis_available(
    containing_size: Pixels,
    start: Option<Pixels>,
    end: Option<Pixels>,
    margin_start: Pixels,
    margin_end: Pixels,
) -> Pixels {
    (containing_size
        - start.unwrap_or(px(0))
        - end.unwrap_or(px(0))
        - margin_start.non_negative()
        - margin_end.non_negative())
    .non_negative()
}

fn absolute_axis_origin(
    containing_start: Pixels,
    containing_size: Pixels,
    child_size: Pixels,
    start: Option<Pixels>,
    end: Option<Pixels>,
    margin_start: Pixels,
    margin_end: Pixels,
) -> Pixels {
    let margin_start = margin_start.non_negative();
    let margin_end = margin_end.non_negative();

    if let Some(start) = start {
        containing_start + start + margin_start
    } else if let Some(end) = end {
        containing_start + containing_size - end - margin_end - child_size
    } else {
        containing_start + margin_start
    }
}

fn relative_axis_offset(start: Option<Pixels>, end: Option<Pixels>) -> Pixels {
    if let Some(start) = start {
        start
    } else if let Some(end) = end {
        px(0) - end
    } else {
        px(0)
    }
}

fn relative_offset(style: Style) -> Offset {
    Offset::new(
        relative_axis_offset(style.inset.left, style.inset.right),
        relative_axis_offset(style.inset.top, style.inset.bottom),
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
    fn flow_child_count(&self, parent: NodeId) -> usize {
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

    fn node_position(&self, node: NodeId) -> Position {
        self.node_flex_style(node)
            .map(|style| style.position)
            .unwrap_or(Position::Static)
    }

    fn has_absolute_children(&self, parent: NodeId) -> bool {
        let mut current = self.node(parent).first_child;

        while let Some(child) = current {
            if self.node_position(child) == Position::Absolute {
                return true;
            }

            current = self.node(child).next_sibling;
        }

        false
    }

    fn measure_node(
        &mut self,
        node: NodeId,
        available: Size,
        text_measurer: &dyn TextMeasurer,
    ) -> Size {
        count_metric!(self, measure_node_calls);

        if let Some(measured) = self.cached_measurement(node, available) {
            count_metric!(self, measurement_cache_hits);

            return measured;
        }

        count_metric!(self, measurement_cache_misses);
        let measured = match self.node(node).kind {
            NodeKind::Text { text } => {
                count_metric!(self, text_measurements);
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
            NodeKind::Image { source, style } => measure_image(source, style, available),
            NodeKind::Canvas { style, .. } => measure_canvas(style, available),
            NodeKind::Entity { .. } => match self.node(node).first_child {
                Some(child) => self.measure_node(child, available, text_measurer),
                None => Size::ZERO,
            },
            NodeKind::Div { .. } => {
                let style = self.node(node).style().expect("div node must have style");
                self.measure_div(node, style, available, text_measurer)
            }
        };

        self.cache_measurement(node, available, measured);

        measured
    }

    fn measure_div(
        &mut self,
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
        let child_count = self.flow_child_count(node);
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
            if self.node_position(child) == Position::Absolute {
                current = self.node(child).next_sibling;
                continue;
            }

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
        let initial_containing_block = Rect::new(origin, available);
        let subtree_bounds = self.layout_node_with_size(
            node,
            origin,
            measured,
            initial_containing_block,
            text_measurer,
        );

        // `layout_node()` is the top-level layout entry. There is no parent that will
        // store the root's cache entry for us
        self.set_subtree_paint_bounds(node, subtree_bounds);

        measured
    }

    #[allow(clippy::too_many_arguments)]
    fn layout_div_children(
        &mut self,
        node: NodeId,
        style: Style,
        origin: Point,
        outer_size: Size,
        containing_block: Rect,
        text_measurer: &dyn TextMeasurer,
        mut subtree_bounds: Rect,
    ) -> Rect {
        if self.node(node).first_child.is_none() {
            return subtree_bounds;
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

        // if children are clipped, every non-empty child subtree contributes
        // at most this viewport to the parent's own conservative subtree extent.
        let children_clip = if self.node_clips_children(node) {
            Some(self.children_clip_layout_bounds(node))
        } else {
            None
        };

        let flow_child_count = self.flow_child_count(node);
        let has_absolute_children = self.has_absolute_children(node);
        let gap = style.gap.non_negative();

        let total_gap = if flow_child_count > 1 {
            let gap_count = i32::try_from(flow_child_count - 1).unwrap_or(i32::MAX);
            gap * gap_count
        } else {
            px(0)
        };

        let available_main = main_size(viewport_content_size, axis);
        let available_cross = cross_size(viewport_content_size, axis);

        let totals = if scrolling_main {
            None
        } else {
            Some(self.flex_totals(node, axis, child_available, text_measurer))
        };

        let mut occupied_main = total_gap;
        let mut grow_before = 0u64;
        let mut shrink_before = 0u64;
        let mut current = self.node(node).first_child;

        while let Some(child) = current {
            if self.node_position(child) == Position::Absolute {
                current = self.node(child).next_sibling;
                continue;
            }

            let margin = self.node_margin(child);
            let base = self.flex_base_main_size(child, axis, child_available, text_measurer);
            let target_main = self.flex_item_main_size(
                child,
                axis,
                base,
                available_main,
                total_gap,
                grow_before,
                shrink_before,
                totals,
            );

            occupied_main += target_main + main_margin_total(margin, axis);
            grow_before = grow_before.saturating_add(self.node_flex_grow(child, axis) as u64);

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
                if flow_child_count > 1 {
                    let spaces = i32::try_from(flow_child_count - 1).unwrap_or(i32::MAX);
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
        let mut flow_index = 0;
        let mut current = self.node(node).first_child;
        let mut sibling_prefix_bounds = Rect::new(Point::ZERO, Size::ZERO);

        while let Some(child) = current {
            let next = self.node(child).next_sibling;
            if self.node_position(child) == Position::Absolute {
                current = next;
                continue;
            }

            let margin = self.node_margin(child);
            let base = self.flex_base_main_size(child, axis, child_available, text_measurer);
            let target_main = self.flex_item_main_size(
                child,
                axis,
                base,
                available_main,
                total_gap,
                grow_before,
                shrink_before,
                totals,
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

            // the child returns its exact conservative subtree extent.
            // `layout_node_with_size()` no longer stores the child's root cache
            // itself. We decide below whether this node needs an exact extent
            // or a cumulative sibling prefix.
            let child_subtree = self.layout_node_with_size(
                child,
                child_origin,
                child_size,
                containing_block,
                text_measurer,
            );

            sibling_prefix_bounds = sibling_prefix_bounds.union(child_subtree);

            if has_absolute_children {
                // Once absolute siblings exist, physical sibling order no longer
                // describes monotonic layout order
                self.set_subtree_paint_bounds(child, child_subtree);
            } else if next.is_some() {
                // non-last sibling: store a cumulative prefix extent. It is still
                // conservative for this child's own subtree
                self.set_subtree_paint_bounds(child, sibling_prefix_bounds);
            } else {
                // last sibling: keep the exact subtree extent. Binary search treats
                // the last child as the fallback result rather than as a prefix
                // predicate entry.
                self.set_subtree_paint_bounds(child, child_subtree);
            }

            if child_subtree.has_area() {
                let contribution = children_clip.unwrap_or(child_subtree);
                subtree_bounds = subtree_bounds.union(contribution);
            }

            cursor += main_margin_start(margin, axis) + target_main + main_margin_end(margin, axis);

            flow_index += 1;
            let has_more_flow_children = flow_index < flow_child_count;

            if has_more_flow_children {
                cursor += gap + between_extra;
                if between_remainder.is_positive() {
                    cursor += px(1);
                    between_remainder -= px(1);
                }
            }

            grow_before = grow_before.saturating_add(self.node_flex_grow(child, axis) as u64);

            let shrink_factor = (self.node_flex_shrink(child, axis) as u64)
                .saturating_mul(base.non_negative().get() as u64);

            shrink_before = shrink_before.saturating_add(shrink_factor);
            current = next;
        }

        self.layout_absolute_children(
            node,
            containing_block,
            children_clip,
            text_measurer,
            subtree_bounds,
        )
    }

    fn layout_absolute_child(
        &mut self,
        child: NodeId,
        containing_block: Rect,
        text_measurer: &dyn TextMeasurer,
    ) -> Rect {
        let style = self
            .node_flex_style(child)
            .expect("absolute node must resolve to a styled element");

        let margin = self.node_margin(child);

        let horizontal_available = absolute_axis_available(
            containing_block.width(),
            style.inset.left,
            style.inset.right,
            margin.left,
            margin.right,
        );
        let vertical_available = absolute_axis_available(
            containing_block.height(),
            style.inset.top,
            style.inset.bottom,
            margin.top,
            margin.bottom,
        );

        let available = Size::new(horizontal_available, vertical_available);
        let measured = self.measure_node(child, available, text_measurer);

        let mut child_size = measured;

        if style.width == Length::Auto && style.inset.left.is_some() && style.inset.right.is_some()
        {
            child_size.width = resolve_dimension(
                Length::Pixels(horizontal_available),
                style.min_width,
                style.max_width,
                horizontal_available,
                horizontal_available,
            );
        }
        if style.height == Length::Auto && style.inset.top.is_some() && style.inset.bottom.is_some()
        {
            child_size.height = resolve_dimension(
                Length::Pixels(vertical_available),
                style.min_height,
                style.max_height,
                vertical_available,
                vertical_available,
            );
        }

        let child_origin = Point::new(
            absolute_axis_origin(
                containing_block.x(),
                containing_block.width(),
                child_size.width,
                style.inset.left,
                style.inset.right,
                margin.left,
                margin.right,
            ),
            absolute_axis_origin(
                containing_block.y(),
                containing_block.height(),
                child_size.height,
                style.inset.top,
                style.inset.bottom,
                margin.top,
                margin.bottom,
            ),
        );

        self.layout_node_with_size(
            child,
            child_origin,
            child_size,
            containing_block,
            text_measurer,
        )
    }

    fn layout_absolute_children(
        &mut self,
        parent: NodeId,
        containing_block: Rect,
        children_clip: Option<Rect>,
        text_measurer: &dyn TextMeasurer,
        mut subtree_bounds: Rect,
    ) -> Rect {
        let mut current = self.node(parent).first_child;

        while let Some(child) = current {
            let next = self.node(child).next_sibling;
            if self.node_position(child) == Position::Absolute {
                let child_subtree =
                    self.layout_absolute_child(child, containing_block, text_measurer);
                self.set_subtree_paint_bounds(child, child_subtree);

                if child_subtree.has_area() {
                    let contribution = children_clip.unwrap_or(child_subtree);
                    subtree_bounds = subtree_bounds.union(contribution);
                }
            }

            current = next;
        }

        subtree_bounds
    }

    pub fn layout(
        &mut self,
        root: NodeId,
        viewport: Size,
        text_measurer: &dyn TextMeasurer,
    ) -> Size {
        self.resolve_text_styles(root);
        self.clear_measurement_caches();

        let size = self.layout_node(root, Point::ZERO, viewport, text_measurer);

        // all subtree extents were produced bottom-up as part of recursive layout.
        self.finish_subtree_paint_bounds();

        size
    }

    fn node_margin(&self, node: NodeId) -> Edges<Pixels> {
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

    fn node_flex_style(&self, node: NodeId) -> Option<Style> {
        match self.node(node).kind {
            NodeKind::Div { .. } => self.node(node).style(),
            NodeKind::Text { .. } | NodeKind::Image { .. } | NodeKind::Canvas { .. } => None,
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

    fn flex_totals(
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
    fn flex_item_main_size(
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

    fn layout_node_with_size(
        &mut self,
        node: NodeId,
        origin: Point,
        size: Size,
        containing_block: Rect,
        text_measurer: &dyn TextMeasurer,
    ) -> Rect {
        count_metric!(self, nodes_laid_out);

        let direct_style = match self.node(node).kind {
            NodeKind::Div { .. } => self.node(node).style(),
            NodeKind::Text { .. }
            | NodeKind::Image { .. }
            | NodeKind::Canvas { .. }
            | NodeKind::Entity { .. } => None,
        };
        let positioned_origin = match direct_style {
            Some(style) if style.position == Position::Relative => origin + relative_offset(style),
            _ => origin,
        };

        self.node_mut(node).layout.bounds = Rect::new(positioned_origin, size);
        let own_bounds = self.own_paint_bounds(node);

        match self.node(node).kind {
            NodeKind::Text { .. } | NodeKind::Image { .. } | NodeKind::Canvas { .. } => own_bounds,
            NodeKind::Entity { .. } => {
                match self.node(node).first_child {
                    Some(child) => {
                        let child_subtree = self.layout_node_with_size(
                            child,
                            origin,
                            size,
                            containing_block,
                            text_measurer,
                        );

                        // entity nodes are layout-transparent. Usually the rendered
                        // child's bounds are identical to the entity's provisional
                        // bounds, but relative positioning can move the rendered root.
                        // keep the entity's bounds synchronized with that root
                        let child_bounds = self.node(child).layout.bounds;
                        self.node_mut(node).layout.bounds = child_bounds;

                        self.set_subtree_paint_bounds(child, child_subtree);

                        child_subtree
                    }
                    None => own_bounds,
                }
            }
            NodeKind::Div { .. } => {
                let style = self.node(node).style().expect("div node must have style");

                // a positioned node establishes the containing block for absolute
                // descendants at its final, visually shifted position.
                // relative positioning therefore moves both the node itself and the
                // coordinate system used by absolute descendants
                let descendant_containing_block = match style.position {
                    Position::Static => containing_block,
                    Position::Relative | Position::Absolute => {
                        content_rect(style, positioned_origin, size)
                    }
                };

                self.layout_div_children(
                    node,
                    style,
                    positioned_origin,
                    size,
                    descendant_containing_block,
                    text_measurer,
                    own_bounds,
                )
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
        let viewport = self.children_clip_layout_bounds(node);
        let style = self.node(node).style();

        let mut right = viewport.right();
        let mut bottom = viewport.bottom();
        let mut has_children = false;
        let mut current = self.node(node).first_child;

        while let Some(child) = current {
            has_children = true;

            let extent = self.scroll_layout_extent(child);

            right = right.max(extent.right());
            bottom = bottom.max(extent.bottom());
            current = self.node(child).next_sibling;
        }

        // child origins already include leading padding. Trailing padding still
        // belongs after the final content extent.
        // don't make an otherwise empty container scroll just because it has padding.
        if has_children && let Some(style) = style {
            right += style.padding.right.non_negative();
            bottom += style.padding.bottom.non_negative();
        }

        Offset::new(
            (right - viewport.right()).non_negative(),
            (bottom - viewport.bottom()).non_negative(),
        )
    }

    fn scroll_layout_extent(&self, node: NodeId) -> Rect {
        let bounds = self.node(node).layout.bounds;
        let margin = self.node_margin(node);

        // left/top margins are already reflected in layout origin.
        // right/bottom margins occupy trailing scrollable space
        let mut extent = Rect::new(
            bounds.origin,
            Size::new(
                bounds.width() + margin.right.non_negative(),
                bounds.height() + margin.bottom.non_negative(),
            ),
        );

        let children_clip = if self.node_clips_children(node) {
            Some(self.children_clip_layout_bounds(node))
        } else {
            None
        };

        let mut current = self.node(node).first_child;

        while let Some(child) = current {
            let child_extent = self.scroll_layout_extent(child);
            let contribution = match children_clip {
                Some(clip) => child_extent.intersection(clip),
                None => Some(child_extent),
            };
            if let Some(contribution) = contribution
                && contribution.has_area()
            {
                extent = extent.union(contribution);
            }

            current = self.node(child).next_sibling;
        }

        extent
    }

    fn own_paint_bounds(&self, node: NodeId) -> Rect {
        let empty = Rect::new(Point::ZERO, Size::ZERO);

        match self.node(node).kind {
            NodeKind::Entity { .. } => empty,
            NodeKind::Div { .. }
            | NodeKind::Text { .. }
            | NodeKind::Image { .. }
            | NodeKind::Canvas { .. } => {
                let bounds = self.node(node).layout.bounds;

                if bounds.has_area() { bounds } else { empty }
            }
        }
    }
}

fn measure_image(source: ImageSource, style: ImageStyle, available: Size) -> Size {
    let intrinsic = source.size();
    let natural = match (style.width, style.height) {
        (None, None) => intrinsic,
        (Some(width), None) => {
            let width = width.non_negative();
            let height = intrinsic.height.scale_ratio_floor(width, intrinsic.width);
            Size::new(width, height)
        }
        (None, Some(height)) => {
            let height = height.non_negative();
            let width = intrinsic.width.scale_ratio_floor(height, intrinsic.height);
            Size::new(width, height)
        }
        (Some(width), Some(height)) => Size::new(width.non_negative(), height.non_negative()),
    };

    Size::new(
        natural.width.min(available.width.non_negative()),
        natural.height.min(available.height.non_negative()),
    )
}

fn measure_canvas(style: CanvasStyle, available: Size) -> Size {
    let width = style
        .width
        .unwrap_or(px(0))
        .non_negative()
        .min(available.width.non_negative());
    let height = style
        .height
        .unwrap_or(px(0))
        .non_negative()
        .min(available.height.non_negative());

    Size::new(width, height)
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

    #[test]
    fn image_uses_intrinsic_size() {
        let measurer = TestTextMeasurer::new(8, 10);

        let source = ImageSource::new(ImageId::new(0), Size::new(px(32), px(18)));

        let mut frame = FrameArena::<8, 128>::default();

        let root = frame.mount(div().child(image(source))).unwrap();

        frame.layout(root, Size::new(px(100), px(100)), &measurer);

        let image_node = frame.node(root).first_child.unwrap();

        assert_bounds(frame.bounds(image_node), 0, 0, 32, 18);
    }

    #[test]
    fn image_intrinsic_size_is_clamped_to_available_space() {
        let measurer = TestTextMeasurer::new(8, 10);

        let source = ImageSource::new(ImageId::new(0), Size::new(px(80), px(40)));

        let mut frame = FrameArena::<8, 128>::default();

        let root = frame
            .mount(div().w(px(30)).h(px(20)).child(image(source)))
            .unwrap();

        frame.layout(root, Size::new(px(30), px(20)), &measurer);

        let image_node = frame.node(root).first_child.unwrap();

        assert_bounds(frame.bounds(image_node), 0, 0, 30, 20);
    }

    #[test]
    fn images_participate_in_block_flow() {
        let measurer = TestTextMeasurer::new(8, 10);

        let first = ImageSource::new(ImageId::new(0), Size::new(px(20), px(10)));
        let second = ImageSource::new(ImageId::new(1), Size::new(px(30), px(15)));

        let mut frame = FrameArena::<8, 128>::default();

        let root = frame
            .mount(div().gap(px(4)).child(image(first)).child(image(second)))
            .unwrap();

        frame.layout(root, Size::new(px(100), px(100)), &measurer);

        let first_node = frame.node(root).first_child.unwrap();
        let second_node = frame.node(first_node).next_sibling.unwrap();

        assert_bounds(frame.bounds(first_node), 0, 0, 20, 10);
        assert_bounds(frame.bounds(second_node), 0, 14, 30, 15);
    }

    #[test]
    fn images_participate_in_flex_row_layout() {
        let measurer = TestTextMeasurer::new(8, 10);

        let first = ImageSource::new(ImageId::new(0), Size::new(px(20), px(10)));
        let second = ImageSource::new(ImageId::new(1), Size::new(px(30), px(15)));

        let mut frame = FrameArena::<8, 128>::default();

        let root = frame
            .mount(
                div()
                    .flex()
                    .gap(px(5))
                    .child(image(first))
                    .child(image(second)),
            )
            .unwrap();

        frame.layout(root, Size::new(px(100), px(40)), &measurer);

        let first_node = frame.node(root).first_child.unwrap();
        let second_node = frame.node(first_node).next_sibling.unwrap();

        assert_bounds(frame.bounds(first_node), 0, 0, 20, 10);
        assert_bounds(frame.bounds(second_node), 25, 0, 30, 15);
    }

    #[test]
    fn image_width_preserves_intrinsic_aspect_ratio() {
        let measurer = TestTextMeasurer::new(8, 10);
        let source = ImageSource::new(ImageId::new(0), Size::new(px(40), px(20)));
        let mut frame = FrameArena::<8, 128>::default();

        let root = frame.mount(div().child(image(source).w(px(20)))).unwrap();

        frame.layout(root, Size::new(px(100), px(100)), &measurer);

        let image_node = frame.node(root).first_child.unwrap();

        assert_bounds(frame.bounds(image_node), 0, 0, 20, 10);
    }

    #[test]
    fn image_height_preserves_intrinsic_aspect_ratio() {
        let measurer = TestTextMeasurer::new(8, 10);
        let source = ImageSource::new(ImageId::new(0), Size::new(px(40), px(20)));
        let mut frame = FrameArena::<8, 128>::default();

        let root = frame.mount(div().child(image(source).h(px(10)))).unwrap();

        frame.layout(root, Size::new(px(100), px(100)), &measurer);

        let image_node = frame.node(root).first_child.unwrap();

        assert_bounds(frame.bounds(image_node), 0, 0, 20, 10);
    }

    #[test]
    fn image_can_use_explicit_box_size() {
        let measurer = TestTextMeasurer::new(8, 10);
        let source = ImageSource::new(ImageId::new(0), Size::new(px(40), px(20)));
        let mut frame = FrameArena::<8, 128>::default();

        let root = frame
            .mount(div().child(image(source).size(Size::new(px(30), px(30))).contain()))
            .unwrap();

        frame.layout(root, Size::new(px(100), px(100)), &measurer);

        let image_node = frame.node(root).first_child.unwrap();

        assert_bounds(frame.bounds(image_node), 0, 0, 30, 30);
    }

    fn draw_test_canvas(bounds: Rect, painter: &mut dyn CanvasPainter) {
        painter.fill_rect(
            Rect::new(Point::ZERO, Size::new(bounds.width(), px(4))),
            Color::RED,
        );
        painter.line(
            Point::new(px(0), px(0)),
            Point::new(bounds.width() - px(1), bounds.height() - px(1)),
            px(1),
            Color::GREEN,
        );
        painter.fill_circle(Point::new(px(10), px(10)), px(3), Color::BLUE);
    }

    #[test]
    fn mounts_canvas_as_leaf() {
        let mut frame = FrameArena::<8, 128>::default();

        let root = frame
            .mount(div().child(canvas(draw_test_canvas).size(Size::new(px(40), px(20)))))
            .unwrap();

        assert_eq!(frame.node_count(), 2);

        let canvas_node = frame.node(root).first_child.unwrap();

        assert_eq!(frame.node(canvas_node).parent, Some(root));
        assert!(matches!(
            frame.node(canvas_node).kind,
            NodeKind::Canvas { .. }
        ));
        assert_eq!(frame.node(canvas_node).first_child, None);
    }

    #[test]
    fn canvas_uses_explicit_size() {
        let measurer = TestTextMeasurer::new(8, 10);

        let mut frame = FrameArena::<8, 128>::default();

        let root = frame
            .mount(div().child(canvas(draw_test_canvas).size(Size::new(px(40), px(20)))))
            .unwrap();

        frame.layout(root, Size::new(px(100), px(100)), &measurer);

        let canvas_node = frame.node(root).first_child.unwrap();

        assert_bounds(frame.bounds(canvas_node), 0, 0, 40, 20);
    }

    #[test]
    fn canvas_size_is_clamped_to_available_space() {
        let measurer = TestTextMeasurer::new(8, 10);

        let mut frame = FrameArena::<8, 128>::default();

        let root = frame
            .mount(
                div()
                    .w(px(30))
                    .h(px(15))
                    .child(canvas(draw_test_canvas).size(Size::new(px(100), px(50)))),
            )
            .unwrap();

        frame.layout(root, Size::new(px(30), px(15)), &measurer);

        let canvas_node = frame.node(root).first_child.unwrap();

        assert_bounds(frame.bounds(canvas_node), 0, 0, 30, 15);
    }

    #[test]
    fn fixed_main_length_does_not_require_intrinsic_measurement() {
        struct PanicTextMeasurer;
        impl TextMeasurer for PanicTextMeasurer {
            fn measure_text(&self, _: &str, _: ResolvedTextStyle, _: Size) -> Size {
                panic!("fixed main length should not require intrinsic measurement");
            }
        }

        let mut frame = FrameArena::<16, 128>::default();

        let node = frame
            .mount(
                div()
                    .w(px(100))
                    .h(px(80))
                    .child("this must not be measured"),
            )
            .unwrap();

        let base = frame.flex_base_main_size(
            node,
            super::Axis::Vertical,
            Size::new(px(100), px(100)),
            &PanicTextMeasurer,
        );

        assert_eq!(base, px(80),);
    }

    #[test]
    fn layout_caches_cumulative_paint_bounds_for_ordered_sibling_prefixes() {
        let measurer = TestTextMeasurer::new(8, 10);

        let mut frame = FrameArena::<32, 128>::default();

        let root = frame
            .mount(
                div()
                    .w(px(100))
                    .h(px(30))
                    .child(
                        div()
                            .w(px(100))
                            .h(px(10))
                            .child(div().w(px(100)).h(px(100))),
                    )
                    .child(div().w(px(100)).h(px(10)))
                    .child(div().w(px(100)).h(px(10))),
            )
            .unwrap();

        frame.layout(root, Size::new(px(100), px(100)), &measurer);

        let first = frame.node(root).first_child.unwrap();
        let second = frame.node(first).next_sibling.unwrap();
        let third = frame.node(second).next_sibling.unwrap();

        // first child's own subtree overflows to y=100.
        assert_eq!(
            frame.ordered_prefix_paint_bounds(first,).unwrap().bottom(),
            px(100),
        );
        // second child's own box ends at y=20, but its cumulative prefix must
        // retain the first sibling's overflow.
        assert_eq!(
            frame.ordered_prefix_paint_bounds(second,).unwrap().bottom(),
            px(100),
        );
        // last child is deliberately not a prefix entry.
        assert_eq!(frame.ordered_prefix_paint_bounds(third,), None,);
        // its normal conservative subtree cache stays exact.
        assert_eq!(frame.subtree_paint_bounds(third,).unwrap().bottom(), px(30),);
    }

    #[test]
    fn absolute_child_is_removed_from_block_flow() {
        let measurer = TestTextMeasurer::new(8, 10);
        let mut frame = FrameArena::<16, 128>::default();

        let root = frame
            .mount(
                div()
                    .relative()
                    .w(px(100))
                    .h(px(100))
                    .gap(px(5))
                    .child(div().w(px(20)).h(px(10)))
                    .child(
                        div()
                            .absolute()
                            .top(px(40))
                            .left(px(40))
                            .w(px(30))
                            .h(px(30)),
                    )
                    .child(div().w(px(20)).h(px(10))),
            )
            .unwrap();

        frame.layout(root, Size::new(px(100), px(100)), &measurer);

        let first = frame.node(root).first_child.unwrap();
        let absolute = frame.node(first).next_sibling.unwrap();
        let second = frame.node(absolute).next_sibling.unwrap();

        assert_bounds(frame.bounds(first), 0, 0, 20, 10);
        assert_bounds(frame.bounds(second), 0, 15, 20, 10);
        assert_bounds(frame.bounds(absolute), 40, 40, 30, 30);
    }

    #[test]
    fn absolute_child_does_not_contribute_to_auto_parent_size() {
        let measurer = TestTextMeasurer::new(8, 10);
        let mut frame = FrameArena::<16, 128>::default();

        let root = frame
            .mount(
                div()
                    .relative()
                    .gap(px(4))
                    .child(div().w(px(20)).h(px(10)))
                    .child(div().absolute().w(px(90)).h(px(90)))
                    .child(div().w(px(30)).h(px(15))),
            )
            .unwrap();

        let size = frame.layout(root, Size::new(px(200), px(200)), &measurer);

        assert_eq!(size, Size::new(px(30), px(29)),);
    }

    #[test]
    fn absolute_child_uses_positioned_ancestor_content_box() {
        let measurer = TestTextMeasurer::new(8, 10);
        let mut frame = FrameArena::<16, 128>::default();

        let root = frame
            .mount(
                div()
                    .relative()
                    .w(px(100))
                    .h(px(80))
                    .border(px(2))
                    .p(px(10))
                    .child(div().absolute().top(px(3)).right(px(4)).w(px(20)).h(px(10))),
            )
            .unwrap();

        frame.layout(root, Size::new(px(100), px(80)), &measurer);

        let child = frame.node(root).first_child.unwrap();

        // root content box:
        //   origin = 2 border + 10 padding = (12, 12)
        //   width  = 100 - 4 border - 20 padding = 76
        //   height = 80  - 4 border - 20 padding = 56
        //
        // right: 4 => x = 12 + 76 - 4 - 20 = 64
        // top:   3 => y = 12 + 3 = 15
        assert_bounds(frame.bounds(child), 64, 15, 20, 10);
    }

    #[test]
    fn opposing_absolute_insets_stretch_auto_size() {
        let measurer = TestTextMeasurer::new(8, 10);
        let mut frame = FrameArena::<16, 128>::default();

        let root = frame
            .mount(
                div().relative().w(px(100)).h(px(60)).child(
                    div()
                        .absolute()
                        .left(px(10))
                        .right(px(15))
                        .top(px(5))
                        .bottom(px(7)),
                ),
            )
            .unwrap();

        frame.layout(root, Size::new(px(100), px(60)), &measurer);

        let child = frame.node(root).first_child.unwrap();

        assert_bounds(frame.bounds(child), 10, 5, 75, 48);
    }

    #[test]
    fn absolute_descendant_uses_nearest_positioned_ancestor() {
        let measurer = TestTextMeasurer::new(8, 10);
        let mut frame = FrameArena::<16, 128>::default();

        let root = frame
            .mount(
                div().relative().w(px(100)).h(px(100)).child(
                    div().w(px(20)).h(px(20)).child(
                        div()
                            .absolute()
                            .right(px(5))
                            .bottom(px(6))
                            .w(px(10))
                            .h(px(10)),
                    ),
                ),
            )
            .unwrap();

        frame.layout(root, Size::new(px(100), px(100)), &measurer);

        let static_child = frame.node(root).first_child.unwrap();
        let absolute = frame.node(static_child).first_child.unwrap();

        assert_bounds(frame.bounds(static_child), 0, 0, 20, 20);
        assert_bounds(frame.bounds(absolute), 85, 84, 10, 10);
    }

    #[test]
    fn absolute_child_does_not_participate_in_flex_distribution() {
        let measurer = TestTextMeasurer::new(8, 10);
        let mut frame = FrameArena::<16, 128>::default();

        let root = frame
            .mount(
                div()
                    .relative()
                    .flex()
                    .w(px(100))
                    .h(px(30))
                    .child(div().w(px(20)).h(px(10)))
                    .child(div().absolute().w(px(90)).h(px(20)))
                    .child(div().flex_1().h(px(10))),
            )
            .unwrap();

        frame.layout(root, Size::new(px(100), px(30)), &measurer);

        let fixed = frame.node(root).first_child.unwrap();
        let absolute = frame.node(fixed).next_sibling.unwrap();
        let flexible = frame.node(absolute).next_sibling.unwrap();

        assert_bounds(frame.bounds(fixed), 0, 0, 20, 10);
        assert_bounds(frame.bounds(flexible), 20, 0, 80, 10);
        assert_bounds(frame.bounds(absolute), 0, 0, 90, 20);
    }

    #[test]
    fn relative_offset_preserves_normal_flow_slot() {
        let measurer = TestTextMeasurer::new(8, 10);
        let mut frame = FrameArena::<16, 128>::default();

        let root = frame
            .mount(
                div()
                    .w(px(100))
                    .h(px(100))
                    .gap(px(4))
                    .child(div().relative().left(px(5)).top(px(7)).w(px(20)).h(px(10)))
                    .child(div().w(px(20)).h(px(10))),
            )
            .unwrap();

        frame.layout(root, Size::new(px(100), px(100)), &measurer);

        let first = frame.node(root).first_child.unwrap();
        let second = frame.node(first).next_sibling.unwrap();

        assert_bounds(frame.bounds(first), 5, 7, 20, 10);

        // the first child still occupies its original y=0..10 flow slot.
        // therefore the second child starts at 10 + 4, not 7 + 10 + 4.
        assert_bounds(frame.bounds(second), 0, 14, 20, 10);
    }

    #[test]
    fn relative_right_and_bottom_offset_in_negative_direction() {
        let measurer = TestTextMeasurer::new(8, 10);
        let mut frame = FrameArena::<16, 128>::default();

        let root = frame
            .mount(
                div().w(px(100)).h(px(100)).child(
                    div()
                        .relative()
                        .right(px(6))
                        .bottom(px(8))
                        .w(px(20))
                        .h(px(10)),
                ),
            )
            .unwrap();

        frame.layout(root, Size::new(px(100), px(100)), &measurer);

        let child = frame.node(root).first_child.unwrap();

        assert_bounds(frame.bounds(child), -6, -8, 20, 10);
    }

    #[test]
    fn relative_left_and_top_take_precedence_over_opposing_insets() {
        let measurer = TestTextMeasurer::new(8, 10);
        let mut frame = FrameArena::<16, 128>::default();

        let root = frame
            .mount(
                div().w(px(100)).h(px(100)).child(
                    div()
                        .relative()
                        .left(px(5))
                        .right(px(40))
                        .top(px(3))
                        .bottom(px(30))
                        .w(px(20))
                        .h(px(10)),
                ),
            )
            .unwrap();

        frame.layout(root, Size::new(px(100), px(100)), &measurer);

        let child = frame.node(root).first_child.unwrap();

        assert_bounds(frame.bounds(child), 5, 3, 20, 10);
    }

    #[test]
    fn relative_offset_does_not_change_parent_intrinsic_size() {
        let measurer = TestTextMeasurer::new(8, 10);
        let mut frame = FrameArena::<16, 128>::default();

        let root = frame
            .mount(
                div().child(
                    div()
                        .relative()
                        .left(px(100))
                        .top(px(80))
                        .w(px(20))
                        .h(px(10)),
                ),
            )
            .unwrap();

        let size = frame.layout(root, Size::new(px(200), px(200)), &measurer);

        let child = frame.node(root).first_child.unwrap();

        assert_eq!(size, Size::new(px(20), px(10)),);

        assert_bounds(frame.bounds(root), 0, 0, 20, 10);
        assert_bounds(frame.bounds(child), 100, 80, 20, 10);
    }

    #[test]
    fn absolute_descendant_uses_shifted_relative_containing_block() {
        let measurer = TestTextMeasurer::new(8, 10);
        let mut frame = FrameArena::<16, 128>::default();

        let root = frame
            .mount(
                div().w(px(100)).h(px(100)).child(
                    div()
                        .relative()
                        .left(px(10))
                        .top(px(5))
                        .w(px(40))
                        .h(px(30))
                        .child(
                            div()
                                .absolute()
                                .right(px(0))
                                .bottom(px(0))
                                .w(px(10))
                                .h(px(10)),
                        ),
                ),
            )
            .unwrap();

        frame.layout(root, Size::new(px(100), px(100)), &measurer);

        let relative = frame.node(root).first_child.unwrap();
        let absolute = frame.node(relative).first_child.unwrap();

        assert_bounds(frame.bounds(relative), 10, 5, 40, 30);

        // relative content box:
        // x = 10, width  = 40
        // y = 5,  height = 30
        //
        // absolute 10x10 at right/bottom:
        // x = 10 + 40 - 10 = 40
        // y =  5 + 30 - 10 = 25
        assert_bounds(frame.bounds(absolute), 40, 25, 10, 10);
    }

    #[test]
    fn relative_positioning_can_overlap_following_flow_sibling() {
        let measurer = TestTextMeasurer::new(8, 10);
        let mut frame = FrameArena::<16, 128>::default();

        let root = frame
            .mount(
                div()
                    .w(px(100))
                    .h(px(100))
                    .child(div().relative().top(px(8)).w(px(30)).h(px(10)))
                    .child(div().w(px(30)).h(px(10))),
            )
            .unwrap();

        frame.layout(root, Size::new(px(100), px(100)), &measurer);

        let first = frame.node(root).first_child.unwrap();
        let second = frame.node(first).next_sibling.unwrap();

        assert_bounds(frame.bounds(first), 0, 8, 30, 10);
        assert_bounds(frame.bounds(second), 0, 10, 30, 10);

        assert!(
            frame
                .bounds(first)
                .intersection(frame.bounds(second))
                .is_some()
        );
    }

    #[test]
    fn positioned_descendant_extends_scroll_range_through_static_wrapper() {
        let measurer = TestTextMeasurer::new(8, 10);
        let mut frame = FrameArena::<16, 128>::default();

        let root = frame
            .mount(
                div()
                    .id("scroll")
                    .relative()
                    .w(px(100))
                    .h(px(40))
                    .overflow_y_scroll()
                    .child(
                        div().w(px(100)).h(px(10)).child(
                            div()
                                .absolute()
                                .top(px(80))
                                .left(px(0))
                                .w(px(100))
                                .h(px(20)),
                        ),
                    ),
            )
            .unwrap();

        frame.layout(root, Size::new(px(100), px(40)), &measurer);

        assert_eq!(frame.max_scroll_offset(root), Offset::new(px(0), px(60)),);
    }

    #[test]
    fn nested_scroll_content_does_not_expand_outer_scroll_range() {
        let measurer = TestTextMeasurer::new(8, 10);
        let mut frame = FrameArena::<16, 128>::default();

        let root = frame
            .mount(
                div()
                    .id("outer")
                    .relative()
                    .w(px(100))
                    .h(px(40))
                    .overflow_y_scroll()
                    .child(
                        div()
                            .id("inner")
                            .relative()
                            .w(px(100))
                            .h(px(20))
                            .overflow_y_scroll()
                            .child(
                                div()
                                    .absolute()
                                    .top(px(100))
                                    .left(px(0))
                                    .w(px(100))
                                    .h(px(20)),
                            ),
                    ),
            )
            .unwrap();

        frame.layout(root, Size::new(px(100), px(40)), &measurer);

        let inner = frame.node(root).first_child.unwrap();

        assert_eq!(frame.max_scroll_offset(root), Offset::ZERO,);
        assert_eq!(frame.max_scroll_offset(inner), Offset::new(px(0), px(100)),);
    }

    #[test]
    fn relative_position_moves_entire_descendant_subtree() {
        let measurer = TestTextMeasurer::new(8, 10);
        let mut frame = FrameArena::<16, 128>::default();

        let root = frame
            .mount(
                div().w(px(120)).h(px(60)).child(
                    div()
                        .relative()
                        .left(px(18))
                        .top(px(3))
                        .w(px(80))
                        .h(px(30))
                        .p(px(5))
                        .child("label"),
                ),
            )
            .unwrap();

        frame.layout(root, Size::new(px(120), px(60)), &measurer);

        let relative = frame.node(root).first_child.unwrap();
        let text = frame.node(relative).first_child.unwrap();

        assert_bounds(frame.bounds(relative), 18, 3, 80, 30);

        // parent moved by (+18, +3), then its 5px padding applies.
        // the text must therefore begin at:
        // x = 18 + 5 = 23
        // y =  3 + 5 =  8
        assert_eq!(frame.bounds(text).x(), px(23));
        assert_eq!(frame.bounds(text).y(), px(8));
    }
}
