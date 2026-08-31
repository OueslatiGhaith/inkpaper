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
mod tests;
