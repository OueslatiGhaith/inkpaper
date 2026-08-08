use crate::{
    Display, FlexDirection, FrameArena, Length, NodeId, NodeKind, Point, Rect, Size, Style, px,
};

pub trait TextMeasurer {
    fn measure(&self, text: &str, max_size: Size) -> Size;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Axis {
    Horizontal,
    Vertical,
}

fn non_negative(value: i32) -> i32 {
    value.max(0)
}

fn main_size(size: Size, axis: Axis) -> i32 {
    match axis {
        Axis::Horizontal => size.width.0,
        Axis::Vertical => size.height.0,
    }
}

fn cross_size(size: Size, axis: Axis) -> i32 {
    match axis {
        Axis::Horizontal => size.height.0,
        Axis::Vertical => size.width.0,
    }
}

fn size_from_axes(main: i32, cross: i32, axis: Axis) -> Size {
    match axis {
        Axis::Horizontal => Size::new(px(main), px(cross)),
        Axis::Vertical => Size::new(px(cross), px(main)),
    }
}

fn point_from_axes(main: i32, cross: i32, axis: Axis) -> Point {
    match axis {
        Axis::Horizontal => Point::new(px(main), px(cross)),
        Axis::Vertical => Point::new(px(cross), px(main)),
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

fn resolve_length(length: Length, available: i32, auto_size: i32) -> i32 {
    let available = non_negative(available);
    match length {
        Length::Auto => non_negative(auto_size).min(available),
        Length::Pixels(value) => non_negative(value.0).min(available),
        Length::Fill => available,
    }
}

fn horizontal_padding(style: Style) -> i32 {
    non_negative(style.padding.left.0) + non_negative(style.padding.right.0)
}

fn vertical_padding(style: Style) -> i32 {
    non_negative(style.padding.top.0) + non_negative(style.padding.bottom.0)
}

fn content_available(style: Style, outer_available: Size) -> Size {
    Size::new(
        px(non_negative(
            outer_available.width.0 - horizontal_padding(style),
        )),
        px(non_negative(
            outer_available.height.0 - vertical_padding(style),
        )),
    )
}

impl<const NODES: usize, const TEXT_BYTES: usize> FrameArena<NODES, TEXT_BYTES> {
    fn node_requested_length(&self, node: NodeId, axis: Axis) -> Length {
        match self.node(node).kind {
            crate::NodeKind::Div { style } => requested_length(style, axis),
            crate::NodeKind::Text { .. } => Length::Auto,
            crate::NodeKind::Entity { .. } => match self.node(node).first_child {
                Some(child) => self.node_requested_length(child, axis),
                None => Length::Auto,
            },
        }
    }

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
                let measured = text_measurer.measure(self.text(text), available);

                Size::new(
                    px(non_negative(measured.width.0).min(non_negative(available.width.0))),
                    px(non_negative(measured.height.0).min(non_negative(available.height.0))),
                )
            }
            crate::NodeKind::Entity { .. } => match self.node(node).first_child {
                Some(child) => self.measure_node(child, available, text_measurer),
                None => Size::ZERO,
            },
            crate::NodeKind::Div { style } => {
                self.measure_div(node, style, available, text_measurer)
            }
        }
    }

    fn measure_div(
        &self,
        node: NodeId,
        style: Style,
        available: crate::Size,
        text_measurer: &dyn TextMeasurer,
    ) -> Size {
        let axis = flow_axis(style);

        let width_limit = match style.width {
            Length::Pixels(value) => non_negative(value.0).min(non_negative(available.width.0)),
            Length::Fill => non_negative(available.width.0),
            Length::Auto => non_negative(available.width.0),
        };

        let height_limit = match style.height {
            Length::Pixels(value) => non_negative(value.0).min(non_negative(available.height.0)),
            Length::Fill => non_negative(available.height.0),
            Length::Auto => non_negative(available.height.0),
        };

        let outer_limit = Size::new(px(width_limit), px(height_limit));
        let child_available = content_available(style, outer_limit);
        let child_count = self.child_count(node);
        let gap = non_negative(style.gap.0);
        let available_main = main_size(child_available, axis);

        let total_gap = if child_count > 1 {
            gap.saturating_mul((child_count - 1) as i32)
        } else {
            0
        };

        let mut fixed_main = 0i32;
        let mut fill_count = 0;
        let mut current = self.node(node).first_child;

        while let Some(child) = current {
            if self.node_requested_length(child, axis) == Length::Fill {
                fill_count += 1;
            } else {
                let measured = self.measure_node(child, child_available, text_measurer);
                fixed_main = fixed_main.saturating_add(main_size(measured, axis));
            }

            current = self.node(child).next_sibling;
        }

        let remaining = non_negative(available_main - fixed_main - total_gap);

        let base_fill = if fill_count == 0 {
            0
        } else {
            remaining / fill_count
        };
        let fill_remainder = if fill_count == 0 {
            0
        } else {
            remaining % fill_count
        };

        let mut natural_main = 0i32;
        let mut natural_cross = 0;
        let mut fill_index = 0;
        let mut current = self.node(node).first_child;

        while let Some(child) = current {
            let is_fill = self.node_requested_length(child, axis) == Length::Fill;
            let child_constraint = if is_fill {
                let extra = if fill_index < fill_remainder as usize {
                    1
                } else {
                    0
                };

                fill_index += 1;

                size_from_axes(base_fill + extra, cross_size(child_available, axis), axis)
            } else {
                child_available
            };
            let measured = self.measure_node(child, child_constraint, text_measurer);
            natural_main = natural_main.saturating_add(main_size(measured, axis));
            natural_cross = natural_cross.max(cross_size(measured, axis));
            current = self.node(child).next_sibling;
        }

        if child_count > 1 {
            natural_main = natural_main.saturating_add(total_gap);
        }

        let natural_content = size_from_axes(natural_main, natural_cross, axis);
        let natural_width = natural_content
            .width
            .0
            .saturating_add(horizontal_padding(style));
        let natural_height = natural_content
            .height
            .0
            .saturating_add(vertical_padding(style));

        Size::new(
            px(resolve_length(
                style.width,
                available.width.0,
                natural_width,
            )),
            px(resolve_length(
                style.height,
                available.height.0,
                natural_height,
            )),
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
        self.node_mut(node).layout.bounds = Rect::new(origin, measured);

        let kind = self.node(node).kind;
        match kind {
            NodeKind::Text { .. } => {}
            NodeKind::Entity { .. } => {
                if let Some(child) = self.node(node).first_child {
                    self.layout_node(child, origin, measured, text_measurer);
                }
            }
            NodeKind::Div { style } => {
                self.layout_div_children(node, style, origin, measured, text_measurer);
            }
        }

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
        let axis = flow_axis(style);
        let left = non_negative(style.padding.left.0);
        let top = non_negative(style.padding.top.0);
        let right = non_negative(style.padding.right.0);
        let bottom = non_negative(style.padding.bottom.0);

        let content_size = Size::new(
            px(non_negative(outer_size.width.0 - left - right)),
            px(non_negative(outer_size.height.0 - top - bottom)),
        );
        let content_origin = Point::new(px(origin.x.0 + left), px(origin.y.0 + top));

        let child_count = self.child_count(node);
        if child_count == 0 {
            return;
        }

        let gap = non_negative(style.gap.0);
        let total_gap = if child_count > 1 {
            gap.saturating_mul((child_count - 1) as i32)
        } else {
            0
        };

        let available_main = main_size(content_size, axis);

        let mut fixed_main: i32 = 0;
        let mut fill_count = 0;
        let mut current = self.node(node).first_child;

        while let Some(child) = current {
            if self.node_requested_length(child, axis) == Length::Fill {
                fill_count += 1;
            } else {
                let measured = self.measure_node(child, content_size, text_measurer);
                fixed_main = fixed_main.saturating_add(main_size(measured, axis));
            }

            current = self.node(child).next_sibling;
        }

        let remaining = non_negative(available_main - fixed_main - total_gap);

        let base_fill = if fill_count == 0 {
            0
        } else {
            remaining / fill_count
        };
        let fill_remainder = if fill_count == 0 {
            0
        } else {
            remaining % fill_count
        };
        let content_main_origin = match axis {
            Axis::Horizontal => content_origin.x.0,
            Axis::Vertical => content_origin.y.0,
        };
        let content_cross_origin = match axis {
            Axis::Horizontal => content_origin.y.0,
            Axis::Vertical => content_origin.x.0,
        };

        let mut cursor = content_main_origin;
        let mut fill_index = 0;
        let mut current = self.node(node).first_child;

        while let Some(child) = current {
            let next = self.node(child).next_sibling;
            let is_fill = self.node_requested_length(child, axis) == Length::Fill;

            let child_available = if is_fill {
                let extra = if fill_index < fill_remainder as usize {
                    1
                } else {
                    0
                };

                fill_index += 1;

                size_from_axes(base_fill + extra, cross_size(content_size, axis), axis)
            } else {
                content_size
            };

            let child_origin = point_from_axes(cursor, content_cross_origin, axis);
            let child_size = self.layout_node(child, child_origin, child_available, text_measurer);

            cursor = cursor.saturating_add(main_size(child_size, axis));
            if next.is_some() {
                cursor = cursor.saturating_add(gap);
            }

            current = next;
        }
    }

    pub fn layout(
        &mut self,
        root: NodeId,
        viewport: Size,
        text_measurer: &dyn TextMeasurer,
    ) -> Size {
        self.layout_node(root, Point::ZERO, viewport, text_measurer)
    }
}

#[cfg(test)]
mod tests {
    use crate::*;

    struct TestTextMeasurer {
        character_width: i32,
        line_height: i32,
    }

    impl TestTextMeasurer {
        fn new(character_width: i32, line_height: i32) -> Self {
            Self {
                character_width,
                line_height,
            }
        }
    }

    impl TextMeasurer for TestTextMeasurer {
        fn measure(&self, text: &str, max_size: Size) -> Size {
            let character_count = text.chars().count() as i32;
            let desired_width = character_count.saturating_mul(self.character_width);
            let desired_height = if text.is_empty() { 0 } else { self.line_height };

            Size::new(
                px(desired_width.max(0).min(max_size.width.0.max(0))),
                px(desired_height.max(0).min(max_size.height.0.max(0))),
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
        let root = runtime.rebuild(app).unwrap();

        runtime
            .layout(Size::new(px(100), px(100)), &measurer)
            .unwrap();

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
}
