use super::*;

use crate::{
    CanvasStyle, FrameArena, ImageSource, ImageStyle, NodeId, NodeKind, Position, count_metric,
};

impl<const NODES: usize, const TEXT_BYTES: usize> FrameArena<NODES, TEXT_BYTES> {
    pub(super) fn measure_node(
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
