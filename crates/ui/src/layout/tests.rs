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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

    let root = frame
        .mount(
            div()
                .w(px(100))
                .h(px(60))
                .p(px(10))
                .child(div().w(px(20)).h(px(15))),
            cx,
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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

    let root = frame
        .mount(
            div()
                .w(px(100))
                .h(px(100))
                .gap(px(4))
                .child(div().w(px(20)).h(px(10)))
                .child(div().w(px(30)).h(px(15))),
            cx,
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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

    let root = frame
        .mount(
            div()
                .flex()
                .w(px(100))
                .h(px(40))
                .gap(px(5))
                .child(div().w(px(20)).h(px(10)))
                .child(div().w(px(30)).h(px(10))),
            cx,
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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

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
            cx,
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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

    let root = frame.mount(div().child("abc"), cx).unwrap();

    frame.layout(root, Size::new(px(100), px(100)), &measurer);

    let text = frame.node(root).first_child.unwrap();

    assert_bounds(frame.bounds(text), 0, 0, 24, 10);
    assert_bounds(frame.bounds(root), 0, 0, 24, 10);
}

#[test]
fn padding_and_gap_are_combined_in_column_layout() {
    let measurer = TestTextMeasurer::new(8, 10);
    let mut frame = FrameArena::<16, 128>::default();
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

    let root = frame
        .mount(
            div()
                .p(px(5))
                .gap(px(3))
                .child(div().w(px(20)).h(px(10)))
                .child(div().w(px(30)).h(px(12))),
            cx,
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

    runtime.create_root(App::new).unwrap();
    runtime.rebuild().unwrap();

    runtime
        .layout_with_measurer(Size::new(px(100), px(100)), &measurer)
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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

    let root = frame.mount(div().child("hello"), cx).unwrap();
    let size = frame.layout(root, Size::new(px(320), px(240)), &measurer);

    assert_eq!(size, Size::new(px(40), px(10),));
}

#[test]
fn fill_root_consumes_the_viewport() {
    let measurer = TestTextMeasurer::new(8, 10);
    let mut frame = FrameArena::<16, 128>::default();
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

    let root = frame.mount(div().w_full().h_full(), cx).unwrap();
    let size = frame.layout(root, Size::new(px(320), px(240)), &measurer);

    assert_eq!(size, Size::new(px(320), px(240),));
    assert_bounds(frame.bounds(root), 0, 0, 320, 240);
}

#[test]
fn items_center_centers_children_on_cross_axis() {
    let measurer = TestTextMeasurer::new(8, 10);
    let mut frame = FrameArena::<16, 128>::default();
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

    let root = frame
        .mount(
            div()
                .flex()
                .items_center()
                .w(px(100))
                .h(px(60))
                .child(div().w(px(20)).h(px(20))),
            cx,
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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

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
            cx,
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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

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
            cx,
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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

    let root = frame
        .mount(
            div()
                .flex()
                .w(px(100))
                .h(px(40))
                .child(div().w(px(20)).h(px(10)).ml(px(5)).mr(px(7)))
                .child(div().w(px(20)).h(px(10))),
            cx,
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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

    let root = frame.mount(div().min_w(px(80)).child("Hi"), cx).unwrap();

    frame.layout(root, Size::new(px(200), px(100)), &measurer);

    assert_eq!(frame.bounds(root).width(), px(80));
}

#[test]
fn max_width_limits_auto_element() {
    let measurer = TestTextMeasurer::new(8, 10);
    let mut frame = FrameArena::<16, 128>::default();
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

    let root = frame
        .mount(
            div()
                .max_w(px(40))
                .child("This is much wider than forty pixels"),
            cx,
        )
        .unwrap();

    frame.layout(root, Size::new(px(200), px(100)), &measurer);

    assert_eq!(frame.bounds(root).width(), px(40));
}

#[test]
fn border_width_reduces_content_area() {
    let measurer = TestTextMeasurer::new(8, 10);
    let mut frame = FrameArena::<16, 128>::default();
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

    let root = frame
        .mount(
            div()
                .w(px(100))
                .h(px(60))
                .border(px(2))
                .p(px(4))
                .child(div().w(px(20)).h(px(10))),
            cx,
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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

    let root = frame
        .mount(
            div()
                .flex()
                .w(px(200))
                .h(px(40))
                .child(div().w(px(50)).h(px(20)))
                .child(div().flex_1().h(px(20))),
            cx,
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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

    let root = frame
        .mount(
            div()
                .flex()
                .w(px(200))
                .h(px(40))
                .child(div().flex_basis(px(0)).flex_grow(1).h(px(20)))
                .child(div().flex_basis(px(0)).flex_grow(2).h(px(20)))
                .child(div().flex_basis(px(0)).flex_grow(1).h(px(20))),
            cx,
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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

    let root = frame
        .mount(
            div()
                .flex()
                .w(px(200))
                .h(px(40))
                .child(div().flex_basis(px(50)).flex_grow(1).h(px(20)))
                .child(div().flex_basis(px(100)).flex_grow(1).h(px(20))),
            cx,
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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

    let root = frame
        .mount(
            div()
                .flex()
                .w(px(100))
                .h(px(40))
                .child(div().flex_basis(px(80)).flex_shrink(1).h(px(20)))
                .child(div().flex_basis(px(80)).flex_shrink(1).h(px(20))),
            cx,
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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

    let root = frame
        .mount(
            div()
                .flex()
                .w(px(100))
                .h(px(40))
                .child(div().flex_basis(px(80)).flex_shrink(1).h(px(20)))
                .child(div().flex_basis(px(80)).flex_shrink(3).h(px(20))),
            cx,
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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

    let root = frame
        .mount(
            div()
                .flex()
                .w(px(100))
                .h(px(40))
                .gap(px(10))
                .child(div().flex_1().h(px(20)))
                .child(div().flex_1().h(px(20))),
            cx,
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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

    let root = frame
        .mount(
            div()
                .flex()
                .w(px(100))
                .h(px(40))
                .child(div().flex_1().mx(px(5)).h(px(20)))
                .child(div().flex_1().mx(px(5)).h(px(20))),
            cx,
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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

    let root = frame
        .mount(
            div()
                .flex()
                .w(px(100))
                .h(px(40))
                .child(div().w_full().h(px(20)))
                .child(div().w_full().h(px(20))),
            cx,
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

    runtime.create_root(|_| App).unwrap();

    runtime.rebuild().unwrap();

    let measurer = TestTextMeasurer::new(8, 10);

    runtime
        .layout_with_measurer(Size::new(px(100), px(60)), &measurer)
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

    runtime.create_root(|_| App).unwrap();

    runtime.rebuild().unwrap();
    runtime
        .layout_with_measurer(Size::new(px(100), px(40)), &TestTextMeasurer::new(8, 10))
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

    runtime.create_root(|_| App).unwrap();

    runtime.rebuild().unwrap();

    let measurer = TestTextMeasurer::new(8, 10);

    runtime
        .layout_with_measurer(Size::new(px(100), px(40)), &measurer)
        .unwrap();

    assert!(runtime.scroll_at(Point::new(px(10), px(10),), Offset::new(px(0), px(20),),));

    runtime.take_invalidation();
    runtime.rebuild().unwrap();
    runtime
        .layout_with_measurer(Size::new(px(100), px(40)), &measurer)
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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

    let root = frame
        .mount(
            div()
                .font(FontId::new(3))
                .text_color(Color::GREEN)
                .line_height(px(18))
                .child("Hello"),
            cx,
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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

    let root = frame.mount(div().child(image(source)), cx).unwrap();

    frame.layout(root, Size::new(px(100), px(100)), &measurer);

    let image_node = frame.node(root).first_child.unwrap();

    assert_bounds(frame.bounds(image_node), 0, 0, 32, 18);
}

#[test]
fn image_intrinsic_size_is_clamped_to_available_space() {
    let measurer = TestTextMeasurer::new(8, 10);

    let source = ImageSource::new(ImageId::new(0), Size::new(px(80), px(40)));

    let mut frame = FrameArena::<8, 128>::default();
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

    let root = frame
        .mount(div().w(px(30)).h(px(20)).child(image(source)), cx)
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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

    let root = frame
        .mount(
            div().gap(px(4)).child(image(first)).child(image(second)),
            cx,
        )
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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

    let root = frame
        .mount(
            div()
                .flex()
                .gap(px(5))
                .child(image(first))
                .child(image(second)),
            cx,
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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

    let root = frame
        .mount(div().child(image(source).w(px(20))), cx)
        .unwrap();

    frame.layout(root, Size::new(px(100), px(100)), &measurer);

    let image_node = frame.node(root).first_child.unwrap();

    assert_bounds(frame.bounds(image_node), 0, 0, 20, 10);
}

#[test]
fn image_height_preserves_intrinsic_aspect_ratio() {
    let measurer = TestTextMeasurer::new(8, 10);
    let source = ImageSource::new(ImageId::new(0), Size::new(px(40), px(20)));
    let mut frame = FrameArena::<8, 128>::default();
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

    let root = frame
        .mount(div().child(image(source).h(px(10))), cx)
        .unwrap();

    frame.layout(root, Size::new(px(100), px(100)), &measurer);

    let image_node = frame.node(root).first_child.unwrap();

    assert_bounds(frame.bounds(image_node), 0, 0, 20, 10);
}

#[test]
fn image_can_use_explicit_box_size() {
    let measurer = TestTextMeasurer::new(8, 10);
    let source = ImageSource::new(ImageId::new(0), Size::new(px(40), px(20)));
    let mut frame = FrameArena::<8, 128>::default();
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

    let root = frame
        .mount(
            div().child(image(source).size(Size::new(px(30), px(30))).contain()),
            cx,
        )
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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

    let root = frame
        .mount(
            div().child(canvas(draw_test_canvas).size(Size::new(px(40), px(20)))),
            cx,
        )
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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);
    let root = frame
        .mount(
            div().child(canvas(draw_test_canvas).size(Size::new(px(40), px(20)))),
            cx,
        )
        .unwrap();

    frame.layout(root, Size::new(px(100), px(100)), &measurer);

    let canvas_node = frame.node(root).first_child.unwrap();

    assert_bounds(frame.bounds(canvas_node), 0, 0, 40, 20);
}

#[test]
fn canvas_size_is_clamped_to_available_space() {
    let measurer = TestTextMeasurer::new(8, 10);

    let mut frame = FrameArena::<8, 128>::default();
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

    let root = frame
        .mount(
            div()
                .w(px(30))
                .h(px(15))
                .child(canvas(draw_test_canvas).size(Size::new(px(100), px(50)))),
            cx,
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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

    let node = frame
        .mount(
            div()
                .w(px(100))
                .h(px(80))
                .child("this must not be measured"),
            cx,
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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

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
            cx,
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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

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
            cx,
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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

    let root = frame
        .mount(
            div()
                .relative()
                .gap(px(4))
                .child(div().w(px(20)).h(px(10)))
                .child(div().absolute().w(px(90)).h(px(90)))
                .child(div().w(px(30)).h(px(15))),
            cx,
        )
        .unwrap();

    let size = frame.layout(root, Size::new(px(200), px(200)), &measurer);

    assert_eq!(size, Size::new(px(30), px(29)),);
}

#[test]
fn absolute_child_uses_positioned_ancestor_content_box() {
    let measurer = TestTextMeasurer::new(8, 10);
    let mut frame = FrameArena::<16, 128>::default();
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

    let root = frame
        .mount(
            div()
                .relative()
                .w(px(100))
                .h(px(80))
                .border(px(2))
                .p(px(10))
                .child(div().absolute().top(px(3)).right(px(4)).w(px(20)).h(px(10))),
            cx,
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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

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
            cx,
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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

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
            cx,
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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

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
            cx,
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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

    let root = frame
        .mount(
            div()
                .w(px(100))
                .h(px(100))
                .gap(px(4))
                .child(div().relative().left(px(5)).top(px(7)).w(px(20)).h(px(10)))
                .child(div().w(px(20)).h(px(10))),
            cx,
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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

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
            cx,
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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

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
            cx,
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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

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
            cx,
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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);
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
            cx,
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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

    let root = frame
        .mount(
            div()
                .w(px(100))
                .h(px(100))
                .child(div().relative().top(px(8)).w(px(30)).h(px(10)))
                .child(div().w(px(30)).h(px(10))),
            cx,
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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

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
            cx,
        )
        .unwrap();

    frame.layout(root, Size::new(px(100), px(40)), &measurer);

    assert_eq!(frame.max_scroll_offset(root), Offset::new(px(0), px(60)),);
}

#[test]
fn nested_scroll_content_does_not_expand_outer_scroll_range() {
    let measurer = TestTextMeasurer::new(8, 10);
    let mut frame = FrameArena::<16, 128>::default();
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

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
            cx,
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
    let globals = GlobalArena::<0, 0>::default();
    let cx = AppContext::from_globals(&globals);

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
            cx,
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
