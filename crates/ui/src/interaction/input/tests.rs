
use core::cell::Cell;
use std::rc::Rc;

use crate::*;

type TestRuntime = Runtime<4096, 16, 4096, 32, 64, 512, 32>;

struct TestTextMeasurer;

impl TextMeasurer for TestTextMeasurer {
    fn measure_text(&self, text: &str, _style: ResolvedTextStyle, max_size: Size) -> Size {
        let width = px(i32::try_from(text.chars().count()).unwrap_or(i32::MAX))
            .saturating_mul(6)
            .min(max_size.width.non_negative());

        let height = if text.is_empty() {
            px(0)
        } else {
            px(0).min(max_size.height.non_negative())
        };

        Size::new(width, height)
    }
}

struct Counter {
    clicks: Rc<Cell<u32>>,
}

impl Counter {
    fn clicked(&mut self, _: &ActivateEvent, cx: &mut Context<Self>) {
        self.clicks.set(self.clicks.get().saturating_add(1));
        cx.notify();
    }
}

impl Render for Counter {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        div().w_full().h_full().p(px(10)).child(
            div()
                .id("button")
                .w(px(80))
                .h(px(30))
                .bg(Color::BLUE)
                .on_activate(cx.listener(Self::clicked))
                .child("Button"),
        )
    }
}

#[test]
fn press_and_release_on_same_element_dispatches_click() {
    let clicks = Rc::new(Cell::new(0));
    let mut runtime = TestRuntime::default();

    let counter = runtime
        .create({
            let clicks = clicks.clone();

            move |_| Counter { clicks }
        })
        .unwrap();

    runtime.rebuild(counter).unwrap();
    runtime
        .layout(Size::new(px(200), px(100)), &TestTextMeasurer)
        .unwrap();

    assert!(runtime.begin_activation_at(Point::new(px(20), px(20),)));
    assert!(
        runtime
            .complete_activation_at(Point::new(px(20), px(20),))
            .unwrap()
    );
    assert_eq!(clicks.get(), 1);
    assert!(runtime.is_dirty());
}

#[test]
fn release_outside_pressed_element_does_not_dispatch_click() {
    let clicks = Rc::new(Cell::new(0));
    let mut runtime = TestRuntime::default();

    let counter = runtime
        .create({
            let clicks = clicks.clone();

            move |_| Counter { clicks }
        })
        .unwrap();

    runtime.rebuild(counter).unwrap();
    runtime
        .layout(Size::new(px(200), px(100)), &TestTextMeasurer)
        .unwrap();

    assert!(runtime.begin_activation_at(Point::new(px(20), px(20),)));
    assert_eq!(runtime.take_invalidation(), Invalidation::None);
    assert!(
        !runtime
            .complete_activation_at(Point::new(px(150), px(80),))
            .unwrap()
    );
    assert_eq!(clicks.get(), 0);
    assert_eq!(runtime.take_invalidation(), Invalidation::None);
    assert!(!runtime.is_dirty());
}

#[test]
fn pointer_down_on_noninteractive_space_does_not_capture() {
    let clicks = Rc::new(Cell::new(0));
    let mut runtime = TestRuntime::default();

    let counter = runtime
        .create({
            let clicks = clicks.clone();

            move |_| Counter { clicks }
        })
        .unwrap();

    runtime.rebuild(counter).unwrap();
    runtime
        .layout(Size::new(px(200), px(100)), &TestTextMeasurer)
        .unwrap();

    assert!(!runtime.begin_activation_at(Point::new(px(150), px(80),)));
    assert!(
        !runtime
            .complete_activation_at(Point::new(px(150), px(80),))
            .unwrap()
    );
    assert_eq!(clicks.get(), 0);
}

struct NestedButtons {
    parent_clicks: Rc<Cell<u32>>,
    child_clicks: Rc<Cell<u32>>,
}

impl NestedButtons {
    fn parent_clicked(&mut self, _: &ActivateEvent, cx: &mut Context<Self>) {
        self.parent_clicks
            .set(self.parent_clicks.get().saturating_add(1));
        cx.notify();
    }

    fn child_clicked(&mut self, _: &ActivateEvent, cx: &mut Context<Self>) {
        self.child_clicks
            .set(self.child_clicks.get().saturating_add(1));
        cx.notify();
    }
}

impl Render for NestedButtons {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        let parent_listener = cx.listener(Self::parent_clicked);

        let child_listener = cx.listener(Self::child_clicked);

        div().w_full().h_full().child(
            div()
                .id("parent")
                .w(px(100))
                .h(px(100))
                .on_activate(parent_listener)
                .child(
                    div()
                        .id("child")
                        .w(px(50))
                        .h(px(50))
                        .on_activate(child_listener),
                ),
        )
    }
}

#[test]
fn deepest_topmost_clickable_element_wins() {
    let parent_clicks = Rc::new(Cell::new(0));
    let child_clicks = Rc::new(Cell::new(0));
    let mut runtime = TestRuntime::default();

    let app = runtime
        .create({
            let parent_clicks = parent_clicks.clone();
            let child_clicks = child_clicks.clone();

            move |_| NestedButtons {
                parent_clicks,
                child_clicks,
            }
        })
        .unwrap();

    runtime.rebuild(app).unwrap();
    runtime
        .layout(Size::new(px(200), px(120)), &TestTextMeasurer)
        .unwrap();

    assert!(runtime.begin_activation_at(Point::new(px(20), px(20),)));
    assert!(
        runtime
            .complete_activation_at(Point::new(px(20), px(20),))
            .unwrap()
    );
    assert_eq!(parent_clicks.get(), 0);
    assert_eq!(child_clicks.get(), 1);
}

#[test]
fn press_survives_rebuild_when_element_identity_is_stable() {
    let clicks = Rc::new(Cell::new(0));
    let mut runtime = TestRuntime::default();

    let counter = runtime
        .create({
            let clicks = clicks.clone();

            move |_| Counter { clicks }
        })
        .unwrap();

    runtime.rebuild(counter).unwrap();
    runtime
        .layout(Size::new(px(200), px(100)), &TestTextMeasurer)
        .unwrap();

    assert!(runtime.begin_activation_at(Point::new(px(20), px(20),)));

    runtime.rebuild(counter).unwrap();
    runtime
        .layout(Size::new(px(200), px(100)), &TestTextMeasurer)
        .unwrap();

    assert!(
        runtime
            .complete_activation_at(Point::new(px(20), px(20),))
            .unwrap()
    );
    assert_eq!(clicks.get(), 1);
}

struct FocusApp {
    first: Rc<Cell<u32>>,
    second: Rc<Cell<u32>>,
    third: Rc<Cell<u32>>,
}

impl FocusApp {
    fn first_clicked(&mut self, _: &ActivateEvent, cx: &mut Context<Self>) {
        self.first.set(self.first.get().saturating_add(1));
        cx.notify();
    }

    fn second_clicked(&mut self, _: &ActivateEvent, cx: &mut Context<Self>) {
        self.second.set(self.second.get().saturating_add(1));
        cx.notify();
    }

    fn third_clicked(&mut self, _: &ActivateEvent, cx: &mut Context<Self>) {
        self.third.set(self.third.get().saturating_add(1));
        cx.notify();
    }
}

impl Render for FocusApp {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        let first = cx.listener(Self::first_clicked);
        let second = cx.listener(Self::second_clicked);
        let third = cx.listener(Self::third_clicked);

        div()
            .w_full()
            .h_full()
            .gap(px(5))
            .child(
                div()
                    .id("first")
                    .w(px(80))
                    .h(px(20))
                    .on_activate(first)
                    .child("First"),
            )
            .child(
                div()
                    .id("not-focusable")
                    .w(px(80))
                    .h(px(20))
                    .child("Not interactive"),
            )
            .child(
                div()
                    .id("second")
                    .w(px(80))
                    .h(px(20))
                    .on_activate(second)
                    .child("Second"),
            )
            .child(
                div()
                    .id("third")
                    .w(px(80))
                    .h(px(20))
                    .on_activate(third)
                    .child("Third"),
            )
    }
}

#[test]
fn focus_next_moves_through_clickable_elements() {
    let first = Rc::new(Cell::new(0));
    let second = Rc::new(Cell::new(0));
    let third = Rc::new(Cell::new(0));
    let mut runtime = TestRuntime::default();

    let app = runtime
        .create({
            let first = first.clone();
            let second = second.clone();
            let third = third.clone();

            move |_| FocusApp {
                first,
                second,
                third,
            }
        })
        .unwrap();

    runtime.rebuild(app).unwrap();
    runtime
        .layout(Size::new(px(200), px(200)), &TestTextMeasurer)
        .unwrap();

    assert!(runtime.focus_next());
    assert!(runtime.activate_focused().unwrap());
    assert_eq!(first.get(), 1);
    assert_eq!(second.get(), 0);
    assert_eq!(third.get(), 0);

    runtime.take_dirty();

    assert!(runtime.focus_next());
    assert!(runtime.activate_focused().unwrap());
    assert_eq!(first.get(), 1);
    assert_eq!(second.get(), 1);
    assert_eq!(third.get(), 0);
}

#[test]
fn focus_next_wraps_to_first_element() {
    let first = Rc::new(Cell::new(0));
    let second = Rc::new(Cell::new(0));
    let third = Rc::new(Cell::new(0));
    let mut runtime = TestRuntime::default();

    let app = runtime
        .create({
            let first = first.clone();
            let second = second.clone();
            let third = third.clone();

            move |_| FocusApp {
                first,
                second,
                third,
            }
        })
        .unwrap();

    runtime.rebuild(app).unwrap();
    runtime
        .layout(Size::new(px(200), px(200)), &TestTextMeasurer)
        .unwrap();

    assert!(runtime.focus_next());
    assert!(runtime.focus_next());
    assert!(runtime.focus_next());
    assert!(runtime.focus_next());
    assert!(runtime.activate_focused().unwrap());
    assert_eq!(first.get(), 1);
    assert_eq!(second.get(), 0);
    assert_eq!(third.get(), 0);
}

#[test]
fn focus_previous_wraps_to_last_element() {
    let first = Rc::new(Cell::new(0));
    let second = Rc::new(Cell::new(0));
    let third = Rc::new(Cell::new(0));
    let mut runtime = TestRuntime::default();

    let app = runtime
        .create({
            let first = first.clone();
            let second = second.clone();
            let third = third.clone();

            move |_| FocusApp {
                first,
                second,
                third,
            }
        })
        .unwrap();

    runtime.rebuild(app).unwrap();
    runtime
        .layout(Size::new(px(200), px(200)), &TestTextMeasurer)
        .unwrap();

    assert!(runtime.focus_previous());
    assert!(runtime.activate_focused().unwrap());
    assert_eq!(first.get(), 0);
    assert_eq!(second.get(), 0);
    assert_eq!(third.get(), 1);
}

#[test]
fn focus_survives_rebuild_when_element_still_exists() {
    let first = Rc::new(Cell::new(0));
    let second = Rc::new(Cell::new(0));
    let third = Rc::new(Cell::new(0));
    let mut runtime = TestRuntime::default();

    let app = runtime
        .create({
            let first = first.clone();
            let second = second.clone();
            let third = third.clone();

            move |_| FocusApp {
                first,
                second,
                third,
            }
        })
        .unwrap();

    runtime.rebuild(app).unwrap();
    runtime
        .layout(Size::new(px(200), px(200)), &TestTextMeasurer)
        .unwrap();

    assert!(runtime.focus_next());
    assert!(runtime.focus_next());

    runtime.rebuild(app).unwrap();
    runtime
        .layout(Size::new(px(200), px(200)), &TestTextMeasurer)
        .unwrap();

    assert!(runtime.activate_focused().unwrap());
    assert_eq!(first.get(), 0);
    assert_eq!(second.get(), 1);
    assert_eq!(third.get(), 0);
}

#[test]
fn focused_activation_uses_current_frame_listener() {
    let first = Rc::new(Cell::new(0));
    let second = Rc::new(Cell::new(0));
    let third = Rc::new(Cell::new(0));
    let mut runtime = TestRuntime::default();

    let app = runtime
        .create({
            let first = first.clone();
            let second = second.clone();
            let third = third.clone();

            move |_| FocusApp {
                first,
                second,
                third,
            }
        })
        .unwrap();

    runtime.rebuild(app).unwrap();
    runtime
        .layout(Size::new(px(200), px(200)), &TestTextMeasurer)
        .unwrap();

    assert!(runtime.focus_next());

    runtime.rebuild(app).unwrap();
    runtime
        .layout(Size::new(px(200), px(200)), &TestTextMeasurer)
        .unwrap();

    assert!(runtime.activate_focused().unwrap());
    assert_eq!(first.get(), 1);
}

#[test]
fn successful_pointer_click_establishes_focus() {
    let first = Rc::new(Cell::new(0));
    let second = Rc::new(Cell::new(0));
    let third = Rc::new(Cell::new(0));
    let mut runtime = TestRuntime::default();

    let app = runtime
        .create({
            let first = first.clone();
            let second = second.clone();
            let third = third.clone();

            move |_| FocusApp {
                first,
                second,
                third,
            }
        })
        .unwrap();

    runtime.rebuild(app).unwrap();
    runtime
        .layout(Size::new(px(200), px(200)), &TestTextMeasurer)
        .unwrap();

    assert!(runtime.begin_activation_at(Point::new(px(10), px(55),)));
    assert!(
        runtime
            .complete_activation_at(Point::new(px(10), px(55),))
            .unwrap()
    );
    assert_eq!(second.get(), 1);

    runtime.take_dirty();

    assert!(runtime.focus_next());
    assert!(runtime.activate_focused().unwrap());
    assert_eq!(third.get(), 1);
}

struct EmptyApp;
impl Render for EmptyApp {
    fn render<'a>(&'a mut self, _cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        div().w_full().h_full().child("Nothing interactive")
    }
}

#[test]
fn focus_is_cleared_when_element_disappears() {
    let first = Rc::new(Cell::new(0));
    let second = Rc::new(Cell::new(0));
    let third = Rc::new(Cell::new(0));
    let mut runtime = TestRuntime::default();

    let app = runtime
        .create({
            let first = first.clone();
            let second = second.clone();
            let third = third.clone();

            move |_| FocusApp {
                first,
                second,
                third,
            }
        })
        .unwrap();

    let empty = runtime.create(|_| EmptyApp).unwrap();

    runtime.rebuild(app).unwrap();
    runtime
        .layout(Size::new(px(200), px(200)), &TestTextMeasurer)
        .unwrap();

    assert!(runtime.focus_next());

    runtime.rebuild(empty).unwrap();
    runtime
        .layout(Size::new(px(200), px(200)), &TestTextMeasurer)
        .unwrap();

    assert!(!runtime.activate_focused().unwrap());
}

struct FocusScrollApp;
impl FocusScrollApp {
    fn clicked(&mut self, _: &ActivateEvent, _: &mut Context<Self>) {}
}

impl Render for FocusScrollApp {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        div()
            .id("scroll")
            .w(px(100))
            .h(px(40))
            .overflow_y_scroll()
            .child(
                div()
                    .id("first")
                    .w(px(100))
                    .h(px(30))
                    .on_activate(cx.listener(Self::clicked))
                    .child("First"),
            )
            .child(
                div()
                    .id("second")
                    .w(px(100))
                    .h(px(30))
                    .on_activate(cx.listener(Self::clicked))
                    .child("Second"),
            )
            .child(
                div()
                    .id("third")
                    .w(px(100))
                    .h(px(30))
                    .on_activate(cx.listener(Self::clicked))
                    .child("Third"),
            )
    }
}

#[test]
fn focus_navigation_scrolls_target_into_view() {
    let mut runtime = TestRuntime::default();

    let app = runtime.create(|_| FocusScrollApp).unwrap();

    runtime.rebuild(app).unwrap();
    runtime
        .layout(Size::new(px(100), px(40)), &TestTextMeasurer)
        .unwrap();

    let root = runtime.root_node().unwrap();
    let scroll = runtime.frame().node(root).first_child.unwrap();

    assert!(runtime.focus_next());
    assert_eq!(
        runtime.frame().node(scroll).interaction.scroll_offset,
        Offset::ZERO,
    );

    runtime.take_render_invalidation();

    assert!(runtime.focus_next());
    assert_eq!(
        runtime.frame().node(scroll).interaction.scroll_offset,
        Offset::new(px(0), px(20),),
    );

    let invalidation = runtime.take_render_invalidation();

    assert_eq!(invalidation.kind(), Invalidation::Paint,);
    assert_eq!(
        invalidation.damage().rects(),
        &[Rect::new(
            Point::new(px(0), px(0),),
            Size::new(px(100), px(40),),
        ),],
    );
    assert!(runtime.focus_next());
    assert_eq!(
        runtime.frame().node(scroll).interaction.scroll_offset,
        Offset::new(px(0), px(50),),
    );

    runtime.take_render_invalidation();

    assert!(runtime.focus_previous());
    assert_eq!(
        runtime.frame().node(scroll).interaction.scroll_offset,
        Offset::new(px(0), px(30),),
    );
}

struct NestedFocusScrollApp;
impl NestedFocusScrollApp {
    fn clicked(&mut self, _: &ActivateEvent, _: &mut Context<Self>) {}
}

impl Render for NestedFocusScrollApp {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        div()
            .id("outer")
            .w(px(100))
            .h(px(60))
            .overflow_y_scroll()
            .child(div().w(px(100)).h(px(30)))
            .child(
                div()
                    .id("inner")
                    .w(px(100))
                    .h(px(40))
                    .overflow_y_scroll()
                    .child(
                        div()
                            .id("first")
                            .w(px(100))
                            .h(px(30))
                            .on_activate(cx.listener(Self::clicked))
                            .child("First"),
                    )
                    .child(
                        div()
                            .id("second")
                            .w(px(100))
                            .h(px(30))
                            .on_activate(cx.listener(Self::clicked))
                            .child("Second"),
                    ),
            )
    }
}

#[test]
fn focus_scrolls_nested_containers_inside_out() {
    let mut runtime = TestRuntime::default();

    let app = runtime.create(|_| NestedFocusScrollApp).unwrap();

    runtime.rebuild(app).unwrap();
    runtime
        .layout(Size::new(px(100), px(60)), &TestTextMeasurer)
        .unwrap();

    let root = runtime.root_node().unwrap();
    let outer = runtime.frame().node(root).first_child.unwrap();
    let spacer = runtime.frame().node(outer).first_child.unwrap();
    let inner = runtime.frame().node(spacer).next_sibling.unwrap();

    assert!(runtime.focus_next());

    runtime.take_invalidation();

    assert!(runtime.focus_next());
    assert_eq!(
        runtime.frame().node(inner).interaction.scroll_offset,
        Offset::new(px(0), px(20),)
    );
    assert_eq!(
        runtime.frame().node(outer).interaction.scroll_offset,
        Offset::new(px(0), px(10),)
    );
}

struct LayoutFocusScrollApp;
impl LayoutFocusScrollApp {
    fn clicked(&mut self, _: &ActivateEvent, _: &mut Context<Self>) {}
}

impl Render for LayoutFocusScrollApp {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        div()
            .id("scroll")
            .w(px(100))
            .h(px(40))
            .overflow_y_scroll()
            .child(
                div()
                    .id("first")
                    .w(px(100))
                    .h(px(30))
                    .on_activate(cx.listener(Self::clicked))
                    .child("First"),
            )
            .child(
                div()
                    .id("second")
                    .w(px(100))
                    .h(px(10))
                    .when_focused(|style| style.h(px(30)))
                    .on_activate(cx.listener(Self::clicked))
                    .child("Second"),
            )
    }
}

#[test]
fn focus_scroll_waits_for_layout_when_focus_changes_size() {
    let mut runtime = TestRuntime::default();

    let app = runtime.create(|_| LayoutFocusScrollApp).unwrap();

    runtime.rebuild(app).unwrap();

    let measurer = TestTextMeasurer;

    runtime
        .layout(Size::new(px(100), px(40)), &measurer)
        .unwrap();

    let root = runtime.root_node().unwrap();
    let scroll = runtime.frame().node(root).first_child.unwrap();

    assert!(runtime.focus_next());

    runtime.take_invalidation();

    assert!(runtime.focus_next());
    assert_eq!(runtime.invalidation(), Invalidation::Layout);

    // bounds have not been recalculated yet, therefore scrolling must
    // still be unchanged.
    assert_eq!(
        runtime.frame().node(scroll).interaction.scroll_offset,
        Offset::ZERO
    );

    runtime.take_invalidation();
    runtime
        .layout(Size::new(px(100), px(40)), &measurer)
        .unwrap();

    // second item was 10px tall, but focus changed it to 30px. After layout
    // it occupies y=30..60, so the viewport must scroll by 20px.
    assert_eq!(
        runtime.frame().node(scroll).interaction.scroll_offset,
        Offset::new(px(0), px(20),)
    );
}

struct ExplicitFocusApp;

impl Render for ExplicitFocusApp {
    fn render<'a>(&'a mut self, _cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        div().child(div().id("ignored").w(px(40)).h(px(20))).child(
            div()
                .id("focusable")
                .focusable()
                .w(px(40))
                .h(px(20))
                .when_focused(|style| style.bg(Color::GREEN)),
        )
    }
}

#[test]
fn element_can_be_focusable_without_click_listener() {
    let mut runtime = TestRuntime::default();

    let app = runtime.create(|_| ExplicitFocusApp).unwrap();

    runtime.rebuild(app).unwrap();
    runtime
        .layout(Size::new(px(200), px(100)), &TestTextMeasurer)
        .unwrap();

    assert!(runtime.focus_next());
    assert_eq!(runtime.take_invalidation(), Invalidation::Paint,);
    assert!(!runtime.activate_focused().unwrap());
}

struct ClickRemainsFocusableApp;

impl ClickRemainsFocusableApp {
    fn clicked(&mut self, _: &ActivateEvent, _: &mut Context<Self>) {}
}

impl Render for ClickRemainsFocusableApp {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        div().child(
            div()
                .id("button")
                .w(px(40))
                .h(px(20))
                .on_activate(cx.listener(Self::clicked)),
        )
    }
}

#[test]
fn click_listener_still_makes_element_focusable() {
    let mut runtime = TestRuntime::default();

    let app = runtime.create(|_| ClickRemainsFocusableApp).unwrap();

    runtime.rebuild(app).unwrap();
    runtime
        .layout(Size::new(px(200), px(100)), &TestTextMeasurer)
        .unwrap();

    assert!(runtime.focus_next());
    assert!(runtime.activate_focused().unwrap());
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EncoderTurn {
    delta: i8,
}

struct GenericEventApp;

impl GenericEventApp {
    fn turned(&mut self, _: &EncoderTurn, _: &mut Context<Self>) {}
}

impl Render for GenericEventApp {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        let turn = cx.listener(Self::turned);

        div()
            .id("encoder-target")
            .focusable()
            .on(turn)
            .w(px(80))
            .h(px(30))
    }
}

#[test]
fn arbitrary_event_can_be_bound_to_element() {
    let mut runtime = TestRuntime::default();

    let app = runtime.create(|_| GenericEventApp).unwrap();

    runtime.rebuild(app).unwrap();

    let root = runtime.root_node().unwrap();
    let element = runtime
        .frame()
        .node(root)
        .first_child
        .expect("entity should contain rendered root element");

    let callbacks: std::vec::Vec<_> = runtime
        .frame()
        .event_callbacks(element, core::any::TypeId::of::<EncoderTurn>())
        .collect();

    assert_eq!(callbacks.len(), 1);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EncoderPress;

struct MultipleEventsApp;

impl MultipleEventsApp {
    fn turned(&mut self, _: &EncoderTurn, _: &mut Context<Self>) {}
    fn pressed(&mut self, _: &EncoderPress, _: &mut Context<Self>) {}
}

impl Render for MultipleEventsApp {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        let turn = cx.listener(Self::turned);

        let press = cx.listener(Self::pressed);

        div()
            .id("control")
            .focusable()
            .on(turn)
            .on(press)
            .w(px(80))
            .h(px(30))
    }
}

#[test]
fn element_can_bind_multiple_event_types() {
    let mut runtime = TestRuntime::default();

    let app = runtime.create(|_| MultipleEventsApp).unwrap();

    runtime.rebuild(app).unwrap();

    let root = runtime.root_node().unwrap();
    let element = runtime
        .frame()
        .node(root)
        .first_child
        .expect("entity should contain rendered root element");
    let turns = runtime
        .frame()
        .event_callbacks(element, core::any::TypeId::of::<EncoderTurn>())
        .count();
    let presses = runtime
        .frame()
        .event_callbacks(element, core::any::TypeId::of::<EncoderPress>())
        .count();

    assert_eq!(turns, 1);
    assert_eq!(presses, 1);
}

struct DuplicateEventApp;

impl DuplicateEventApp {
    fn first(&mut self, _: &EncoderTurn, _: &mut Context<Self>) {}
    fn second(&mut self, _: &EncoderTurn, _: &mut Context<Self>) {}
}

impl Render for DuplicateEventApp {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        let first = cx.listener(Self::first);
        let second = cx.listener(Self::second);

        div().id("control").on(first).on(second)
    }
}

#[test]
fn element_can_bind_multiple_handlers_for_same_event() {
    let mut runtime = TestRuntime::default();

    let app = runtime.create(|_| DuplicateEventApp).unwrap();

    runtime.rebuild(app).unwrap();

    let root = runtime.root_node().unwrap();
    let element = runtime
        .frame()
        .node(root)
        .first_child
        .expect("entity should contain rendered root element");

    assert_eq!(
        runtime
            .frame()
            .event_callbacks(element, core::any::TypeId::of::<EncoderTurn>(),)
            .count(),
        2,
    );
}

struct FocusedDispatchApp {
    value: Rc<Cell<i32>>,
}

impl FocusedDispatchApp {
    fn turned(&mut self, event: &EncoderTurn, cx: &mut Context<Self>) {
        self.value.set(self.value.get() + i32::from(event.delta));

        cx.notify();
    }
}

impl Render for FocusedDispatchApp {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        div()
            .id("control")
            .focusable()
            .on(cx.listener(Self::turned))
            .w(px(80))
            .h(px(30))
    }
}

#[test]
fn dispatch_to_focused_invokes_matching_custom_event() {
    let value = Rc::new(Cell::new(0));
    let mut runtime = TestRuntime::default();

    let app = runtime
        .create({
            let value = value.clone();

            move |_| FocusedDispatchApp { value }
        })
        .unwrap();

    runtime.rebuild(app).unwrap();
    runtime
        .layout(Size::new(px(200), px(100)), &TestTextMeasurer)
        .unwrap();

    assert!(runtime.focus_next());

    runtime.take_invalidation();

    assert!(
        runtime
            .dispatch_to_focused(&EncoderTurn { delta: 3 })
            .unwrap()
    );

    assert_eq!(value.get(), 3);
    assert_eq!(runtime.invalidation(), Invalidation::Rebuild);
}

#[test]
fn dispatch_to_focused_ignores_unbound_event_type() {
    let value = Rc::new(Cell::new(0));
    let mut runtime = TestRuntime::default();

    let app = runtime
        .create({
            let value = value.clone();

            move |_| FocusedDispatchApp { value }
        })
        .unwrap();

    runtime.rebuild(app).unwrap();
    runtime
        .layout(Size::new(px(200), px(100)), &TestTextMeasurer)
        .unwrap();

    assert!(runtime.focus_next());

    runtime.take_invalidation();

    assert!(!runtime.dispatch_to_focused(&EncoderPress).unwrap());
    assert_eq!(value.get(), 0);
    assert_eq!(runtime.invalidation(), Invalidation::None);
}

#[test]
fn dispatch_to_focused_without_focus_is_not_handled() {
    let value = Rc::new(Cell::new(0));
    let mut runtime = TestRuntime::default();

    let app = runtime
        .create({
            let value = value.clone();

            move |_| FocusedDispatchApp { value }
        })
        .unwrap();

    runtime.rebuild(app).unwrap();

    assert!(
        !runtime
            .dispatch_to_focused(&EncoderTurn { delta: 1 })
            .unwrap()
    );
    assert_eq!(value.get(), 0);
}

struct MultipleDispatchApp {
    sequence: Rc<Cell<u32>>,
}

impl MultipleDispatchApp {
    fn first(&mut self, _: &EncoderTurn, _: &mut Context<Self>) {
        self.sequence.set(self.sequence.get() * 10 + 1);
    }

    fn second(&mut self, _: &EncoderTurn, _: &mut Context<Self>) {
        self.sequence.set(self.sequence.get() * 10 + 2);
    }
}

impl Render for MultipleDispatchApp {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        let first = cx.listener(Self::first);
        let second = cx.listener(Self::second);

        div().id("control").focusable().on(first).on(second)
    }
}

#[test]
fn dispatch_to_focused_invokes_all_matching_handlers_in_order() {
    let sequence = Rc::new(Cell::new(0));
    let mut runtime = TestRuntime::default();

    let app = runtime
        .create({
            let sequence = sequence.clone();

            move |_| MultipleDispatchApp { sequence }
        })
        .unwrap();

    runtime.rebuild(app).unwrap();

    assert!(runtime.focus_next());
    assert!(
        runtime
            .dispatch_to_focused(&EncoderTurn { delta: 1 })
            .unwrap()
    );
    assert_eq!(sequence.get(), 12);
}

struct FocusRoutingApp {
    first: Rc<Cell<u32>>,
    second: Rc<Cell<u32>>,
}

impl FocusRoutingApp {
    fn first_turned(&mut self, _: &EncoderTurn, _: &mut Context<Self>) {
        self.first.set(self.first.get().saturating_add(1));
    }

    fn second_turned(&mut self, _: &EncoderTurn, _: &mut Context<Self>) {
        self.second.set(self.second.get().saturating_add(1));
    }
}

impl Render for FocusRoutingApp {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        let first = cx.listener(Self::first_turned);
        let second = cx.listener(Self::second_turned);

        div()
            .child(div().id("first").focusable().on(first).w(px(80)).h(px(30)))
            .child(
                div()
                    .id("second")
                    .focusable()
                    .on(second)
                    .w(px(80))
                    .h(px(30)),
            )
    }
}

#[test]
fn dispatch_to_focused_only_invokes_focused_element() {
    let first = Rc::new(Cell::new(0));
    let second = Rc::new(Cell::new(0));

    let mut runtime = TestRuntime::default();

    let app = runtime
        .create({
            let first = first.clone();
            let second = second.clone();

            move |_| FocusRoutingApp { first, second }
        })
        .unwrap();

    runtime.rebuild(app).unwrap();

    assert!(runtime.focus_next());
    assert!(
        runtime
            .dispatch_to_focused(&EncoderTurn { delta: 1 })
            .unwrap()
    );
    assert_eq!(first.get(), 1);
    assert_eq!(second.get(), 0);
    assert!(runtime.focus_next());
    assert!(
        runtime
            .dispatch_to_focused(&EncoderTurn { delta: 1 })
            .unwrap()
    );
    assert_eq!(first.get(), 1);
    assert_eq!(second.get(), 1);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TouchContact {
    pressure: u16,
}

struct SpatialDispatchApp {
    value: Rc<Cell<u16>>,
}

impl SpatialDispatchApp {
    fn touched(&mut self, event: &TouchContact, cx: &mut Context<Self>) {
        self.value.set(event.pressure);
        cx.notify();
    }
}

impl Render for SpatialDispatchApp {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        div().w(px(100)).h(px(100)).p(px(10)).child(
            div()
                .id("target")
                .w(px(40))
                .h(px(30))
                .on(cx.listener(Self::touched)),
        )
    }
}

#[test]
fn dispatch_at_invokes_matching_event_under_position() {
    let value = Rc::new(Cell::new(0));
    let mut runtime = TestRuntime::default();

    let app = runtime
        .create({
            let value = value.clone();

            move |_| SpatialDispatchApp { value }
        })
        .unwrap();

    runtime.rebuild(app).unwrap();
    runtime
        .layout(Size::new(px(100), px(100)), &TestTextMeasurer)
        .unwrap();

    assert!(
        runtime
            .dispatch_at(Point::new(px(20), px(20)), &TouchContact { pressure: 42 },)
            .unwrap()
    );
    assert_eq!(value.get(), 42);
    assert_eq!(runtime.invalidation(), Invalidation::Rebuild);
}

#[test]
fn dispatch_at_outside_target_is_not_handled() {
    let value = Rc::new(Cell::new(0));
    let mut runtime = TestRuntime::default();

    let app = runtime
        .create({
            let value = value.clone();

            move |_| SpatialDispatchApp { value }
        })
        .unwrap();

    runtime.rebuild(app).unwrap();
    runtime
        .layout(Size::new(px(100), px(100)), &TestTextMeasurer)
        .unwrap();

    assert!(
        !runtime
            .dispatch_at(Point::new(px(80), px(80)), &TouchContact { pressure: 42 },)
            .unwrap()
    );
    assert_eq!(value.get(), 0);
}

#[test]
fn dispatch_at_ignores_unbound_event_type() {
    let value = Rc::new(Cell::new(0));
    let mut runtime = TestRuntime::default();

    let app = runtime
        .create({
            let value = value.clone();

            move |_| SpatialDispatchApp { value }
        })
        .unwrap();

    runtime.rebuild(app).unwrap();
    runtime
        .layout(Size::new(px(100), px(100)), &TestTextMeasurer)
        .unwrap();

    assert!(
        !runtime
            .dispatch_at(Point::new(px(20), px(20)), &EncoderPress,)
            .unwrap()
    );

    assert_eq!(value.get(), 0);
}

struct NestedSpatialDispatchApp {
    parent: Rc<Cell<u32>>,
    child: Rc<Cell<u32>>,
}

impl NestedSpatialDispatchApp {
    fn parent_touched(&mut self, _: &TouchContact, _: &mut Context<Self>) {
        self.parent.set(self.parent.get().saturating_add(1));
    }

    fn child_touched(&mut self, _: &TouchContact, _: &mut Context<Self>) {
        self.child.set(self.child.get().saturating_add(1));
    }
}

impl Render for NestedSpatialDispatchApp {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        let parent = cx.listener(Self::parent_touched);
        let child = cx.listener(Self::child_touched);

        div()
            .id("parent")
            .w(px(100))
            .h(px(100))
            .p(px(10))
            .on(parent)
            .child(div().id("child").w(px(40)).h(px(40)).on(child))
    }
}

#[test]
fn dispatch_at_prefers_topmost_matching_element() {
    let parent = Rc::new(Cell::new(0));
    let child = Rc::new(Cell::new(0));

    let mut runtime = TestRuntime::default();

    let app = runtime
        .create({
            let parent = parent.clone();
            let child = child.clone();

            move |_| NestedSpatialDispatchApp { parent, child }
        })
        .unwrap();

    runtime.rebuild(app).unwrap();
    runtime
        .layout(Size::new(px(100), px(100)), &TestTextMeasurer)
        .unwrap();

    assert!(
        runtime
            .dispatch_at(Point::new(px(20), px(20)), &TouchContact { pressure: 1 },)
            .unwrap()
    );
    assert_eq!(parent.get(), 0);
    assert_eq!(child.get(), 1);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ParentOnlyEvent;

struct EventTypeSpatialApp {
    hits: Rc<Cell<u32>>,
}

impl EventTypeSpatialApp {
    fn parent_event(&mut self, _: &ParentOnlyEvent, _: &mut Context<Self>) {
        self.hits.set(self.hits.get().saturating_add(1));
    }
}

impl Render for EventTypeSpatialApp {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        div()
            .id("parent")
            .w(px(100))
            .h(px(100))
            .on(cx.listener(Self::parent_event))
            .child(div().id("child").w(px(50)).h(px(50)))
    }
}

#[test]
fn dispatch_at_can_target_ancestor_when_child_has_no_matching_event() {
    let hits = Rc::new(Cell::new(0));
    let mut runtime = TestRuntime::default();

    let app = runtime
        .create({
            let hits = hits.clone();

            move |_| EventTypeSpatialApp { hits }
        })
        .unwrap();

    runtime.rebuild(app).unwrap();
    runtime
        .layout(Size::new(px(100), px(100)), &TestTextMeasurer)
        .unwrap();

    assert!(
        runtime
            .dispatch_at(Point::new(px(20), px(20)), &ParentOnlyEvent,)
            .unwrap()
    );
    assert_eq!(hits.get(), 1);
}

struct ClippedSpatialApp {
    hits: Rc<Cell<u32>>,
}

impl ClippedSpatialApp {
    fn touched(&mut self, _: &TouchContact, _: &mut Context<Self>) {
        self.hits.set(self.hits.get().saturating_add(1));
    }
}

impl Render for ClippedSpatialApp {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        div().w(px(40)).h(px(40)).overflow_hidden().child(
            div()
                .id("large-child")
                .w(px(80))
                .h(px(80))
                .on(cx.listener(Self::touched)),
        )
    }
}

#[test]
fn dispatch_at_respects_ancestor_clipping() {
    let hits = Rc::new(Cell::new(0));
    let mut runtime = TestRuntime::default();

    let app = runtime
        .create({
            let hits = hits.clone();

            move |_| ClippedSpatialApp { hits }
        })
        .unwrap();

    runtime.rebuild(app).unwrap();
    runtime
        .layout(Size::new(px(100), px(100)), &TestTextMeasurer)
        .unwrap();

    assert!(
        runtime
            .dispatch_at(Point::new(px(20), px(20)), &TouchContact { pressure: 1 },)
            .unwrap()
    );
    assert_eq!(hits.get(), 1);
    assert!(
        !runtime
            .dispatch_at(Point::new(px(60), px(20)), &TouchContact { pressure: 1 },)
            .unwrap()
    );
    assert_eq!(hits.get(), 1);
}

#[test]
fn explicit_target_dispatches_custom_event() {
    let value = Rc::new(Cell::new(0));
    let mut runtime = TestRuntime::default();

    let app = runtime
        .create({
            let value = value.clone();

            move |_| FocusedDispatchApp { value }
        })
        .unwrap();

    runtime.rebuild(app).unwrap();

    assert!(runtime.focus_next());

    let target = runtime
        .focused_target()
        .expect("focused element should have an event target");

    runtime.take_invalidation();

    assert!(
        runtime
            .dispatch_to(target, &EncoderTurn { delta: 4 },)
            .unwrap()
    );
    assert_eq!(value.get(), 4);
}

#[test]
fn event_target_survives_rebuild_when_identity_survives() {
    let value = Rc::new(Cell::new(0));
    let mut runtime = TestRuntime::default();

    let app = runtime
        .create({
            let value = value.clone();

            move |_| FocusedDispatchApp { value }
        })
        .unwrap();

    runtime.rebuild(app).unwrap();

    assert!(runtime.focus_next());

    let target = runtime
        .focused_target()
        .expect("focused element should have an event target");

    runtime.rebuild(app).unwrap();

    assert!(
        runtime
            .dispatch_to(target, &EncoderTurn { delta: 5 },)
            .unwrap()
    );
    assert_eq!(value.get(), 5);
}

struct TargetEmptyApp;

impl Render for TargetEmptyApp {
    fn render<'a>(&'a mut self, _: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        div()
    }
}

#[test]
fn event_target_becomes_stale_when_element_disappears() {
    let value = Rc::new(Cell::new(0));
    let mut runtime = TestRuntime::default();

    let app = runtime
        .create({
            let value = value.clone();

            move |_| FocusedDispatchApp { value }
        })
        .unwrap();

    let empty = runtime.create(|_| TargetEmptyApp).unwrap();

    runtime.rebuild(app).unwrap();

    assert!(runtime.focus_next());

    let old_target = runtime
        .focused_target()
        .expect("focused element should have a target");

    runtime.rebuild(empty).unwrap();

    assert!(
        !runtime
            .dispatch_to(old_target, &EncoderTurn { delta: 1 },)
            .unwrap()
    );
    assert_eq!(value.get(), 0);
}

#[test]
fn stale_event_target_does_not_alias_reappearing_element() {
    let value = Rc::new(Cell::new(0));
    let mut runtime = TestRuntime::default();

    let app = runtime
        .create({
            let value = value.clone();

            move |_| FocusedDispatchApp { value }
        })
        .unwrap();

    let empty = runtime.create(|_| TargetEmptyApp).unwrap();

    runtime.rebuild(app).unwrap();

    assert!(runtime.focus_next());

    let old_target = runtime
        .focused_target()
        .expect("focused element should have a target");

    runtime.rebuild(empty).unwrap();
    runtime.rebuild(app).unwrap();

    assert!(runtime.focus_next());

    let new_target = runtime
        .focused_target()
        .expect("reappearing element should have a target");

    assert_ne!(old_target, new_target);
    assert!(
        !runtime
            .dispatch_to(old_target, &EncoderTurn { delta: 1 },)
            .unwrap()
    );

    assert!(
        runtime
            .dispatch_to(new_target, &EncoderTurn { delta: 2 },)
            .unwrap()
    );
    assert_eq!(value.get(), 2);
}

#[test]
fn target_at_returns_dispatchable_spatial_target() {
    let value = Rc::new(Cell::new(0));
    let mut runtime = TestRuntime::default();

    let app = runtime
        .create({
            let value = value.clone();

            move |_| SpatialDispatchApp { value }
        })
        .unwrap();

    runtime.rebuild(app).unwrap();
    runtime
        .layout(Size::new(px(100), px(100)), &TestTextMeasurer)
        .unwrap();

    let target = runtime
        .target_at::<TouchContact>(Point::new(px(20), px(20)))
        .expect("touch handler should exist at position");

    assert!(
        runtime
            .dispatch_to(target, &TouchContact { pressure: 73 },)
            .unwrap()
    );
    assert_eq!(value.get(), 73);
}

#[test]
fn target_at_is_event_type_specific() {
    let value = Rc::new(Cell::new(0));
    let mut runtime = TestRuntime::default();

    let app = runtime
        .create({
            let value = value.clone();

            move |_| SpatialDispatchApp { value }
        })
        .unwrap();

    runtime.rebuild(app).unwrap();
    runtime
        .layout(Size::new(px(100), px(100)), &TestTextMeasurer)
        .unwrap();

    let position = Point::new(px(20), px(20));

    assert!(runtime.target_at::<TouchContact>(position).is_some());
    assert!(runtime.target_at::<EncoderPress>(position).is_none());
}

#[test]
fn direct_scroll_damages_only_scroll_viewport() {
    let mut runtime = TestRuntime::default();

    let app = runtime.create(|_| FocusScrollApp).unwrap();

    runtime.rebuild(app).unwrap();
    runtime
        .layout(Size::new(px(100), px(40)), &TestTextMeasurer)
        .unwrap();

    assert!(runtime.scroll_at(Point::new(px(10), px(10),), Offset::new(px(0), px(10),),));

    let invalidation = runtime.take_render_invalidation();

    assert_eq!(invalidation.kind(), Invalidation::Paint,);
    assert!(!invalidation.damage().is_full());
    assert_eq!(
        invalidation.damage().rects(),
        &[Rect::new(
            Point::new(px(0), px(0),),
            Size::new(px(100), px(40),),
        ),],
    );
}

#[test]
fn nested_direct_scroll_damage_is_clipped_by_outer_viewport() {
    let mut runtime = TestRuntime::default();

    let app = runtime.create(|_| NestedFocusScrollApp).unwrap();

    runtime.rebuild(app).unwrap();
    runtime
        .layout(Size::new(px(100), px(60)), &TestTextMeasurer)
        .unwrap();

    assert!(runtime.scroll_at(Point::new(px(10), px(35),), Offset::new(px(0), px(10),),));

    let invalidation = runtime.take_render_invalidation();

    assert_eq!(invalidation.kind(), Invalidation::Paint,);
    // inner viewport is y=30..70, but the outer viewport clips it at y=60.
    assert_eq!(
        invalidation.damage().rects(),
        &[Rect::new(
            Point::new(px(0), px(30),),
            Size::new(px(100), px(30),),
        ),],
    );
}

#[test]
fn nested_focus_scroll_damage_collapses_to_outer_viewport() {
    let mut runtime = TestRuntime::default();

    let app = runtime.create(|_| NestedFocusScrollApp).unwrap();

    runtime.rebuild(app).unwrap();
    runtime
        .layout(Size::new(px(100), px(60)), &TestTextMeasurer)
        .unwrap();

    assert!(runtime.focus_next());

    runtime.take_render_invalidation();

    assert!(runtime.focus_next());

    let invalidation = runtime.take_render_invalidation();

    assert_eq!(invalidation.kind(), Invalidation::Paint,);
    // both inner and outer containers scroll.
    // once the outer container moves, its whole 100x60 viewport is damaged
    // and subsumes the nested damage.
    assert_eq!(
        invalidation.damage().rects(),
        &[Rect::new(
            Point::new(px(0), px(0),),
            Size::new(px(100), px(60),),
        ),],
    );
}

struct PositionedHitTestApp {
    clicks: Rc<Cell<u32>>,
}

impl PositionedHitTestApp {
    fn clicked(&mut self, _: &ActivateEvent, _: &mut Context<Self>) {
        self.clicks.set(self.clicks.get().saturating_add(1));
    }
}

impl Render for PositionedHitTestApp {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        div().relative().w(px(100)).h(px(80)).child(
            div().w(px(20)).h(px(20)).child(
                div()
                    .id("button")
                    .absolute()
                    .left(px(50))
                    .top(px(10))
                    .w(px(20))
                    .h(px(20))
                    .on_activate(cx.listener(Self::clicked)),
            ),
        )
    }
}

#[test]
fn absolute_child_is_hittable_outside_unclipped_parent_bounds() {
    let clicks = Rc::new(Cell::new(0));
    let mut runtime = TestRuntime::default();

    let app = runtime
        .create({
            let clicks = clicks.clone();

            move |_| PositionedHitTestApp { clicks }
        })
        .unwrap();

    runtime.rebuild(app).unwrap();
    runtime
        .layout(Size::new(px(100), px(80)), &TestTextMeasurer)
        .unwrap();

    assert!(runtime.begin_activation_at(Point::new(px(55), px(15)),));
    assert!(
        runtime
            .complete_activation_at(Point::new(px(55), px(15)),)
            .unwrap()
    );
    assert_eq!(clicks.get(), 1);
}

struct ClippedPositionedHitTestApp {
    clicks: Rc<Cell<u32>>,
}

impl ClippedPositionedHitTestApp {
    fn clicked(&mut self, _: &ActivateEvent, _: &mut Context<Self>) {
        self.clicks.set(self.clicks.get().saturating_add(1));
    }
}

impl Render for ClippedPositionedHitTestApp {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        div().relative().w(px(100)).h(px(80)).child(
            div().w(px(20)).h(px(20)).overflow_hidden().child(
                div()
                    .id("button")
                    .absolute()
                    .left(px(50))
                    .top(px(10))
                    .w(px(20))
                    .h(px(20))
                    .on_activate(cx.listener(Self::clicked)),
            ),
        )
    }
}

#[test]
fn absolute_child_outside_clipping_parent_is_not_hittable() {
    let clicks = Rc::new(Cell::new(0));
    let mut runtime = TestRuntime::default();

    let app = runtime
        .create({
            let clicks = clicks.clone();

            move |_| ClippedPositionedHitTestApp { clicks }
        })
        .unwrap();

    runtime.rebuild(app).unwrap();
    runtime
        .layout(Size::new(px(100), px(80)), &TestTextMeasurer)
        .unwrap();

    assert!(!runtime.begin_activation_at(Point::new(px(55), px(15)),));
    assert_eq!(clicks.get(), 0);
}

struct AbsoluteFocusScrollApp;

impl Render for AbsoluteFocusScrollApp {
    fn render<'a>(&'a mut self, _cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        div()
            .id("scroll")
            .relative()
            .w(px(100))
            .h(px(40))
            .overflow_y_scroll()
            .child(
                div().w(px(100)).h(px(10)).child(
                    div()
                        .id("target")
                        .absolute()
                        .top(px(80))
                        .left(px(0))
                        .w(px(100))
                        .h(px(20))
                        .focusable(),
                ),
            )
    }
}

#[test]
fn focus_scrolls_nested_absolute_target_into_view() {
    let mut runtime = TestRuntime::default();

    let app = runtime.create(|_| AbsoluteFocusScrollApp).unwrap();

    runtime.rebuild(app).unwrap();
    runtime
        .layout(Size::new(px(100), px(40)), &TestTextMeasurer)
        .unwrap();

    let root = runtime.root_node().unwrap();
    let scroll = runtime.frame().node(root).first_child.unwrap();
    let wrapper = runtime.frame().node(scroll).first_child.unwrap();
    let target = runtime.frame().node(wrapper).first_child.unwrap();

    assert!(runtime.focus_next());
    assert_eq!(
        runtime.frame().node(scroll).interaction.scroll_offset,
        Offset::new(px(0), px(60)),
    );
    assert_eq!(
        runtime.frame().visual_bounds(target),
        Rect::new(Point::new(px(0), px(20)), Size::new(px(100), px(20)),),
    );
}

struct PositionedFocusDamageApp;

impl Render for PositionedFocusDamageApp {
    fn render<'a>(&'a mut self, _cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        div().w(px(100)).h(px(100)).child(
            div()
                .id("target")
                .relative()
                .left(px(10))
                .top(px(5))
                .w(px(20))
                .h(px(10))
                .focusable()
                .bg(Color::BLUE)
                .when_focused(|style| style.bg(Color::GREEN)),
        )
    }
}

#[test]
fn positioned_element_damage_uses_final_visual_bounds() {
    let mut runtime = TestRuntime::default();

    let app = runtime.create(|_| PositionedFocusDamageApp).unwrap();

    runtime.rebuild(app).unwrap();
    runtime
        .layout(Size::new(px(100), px(100)), &TestTextMeasurer)
        .unwrap();

    assert!(runtime.focus_next());

    let invalidation = runtime.take_render_invalidation();

    assert_eq!(invalidation.kind(), Invalidation::Paint,);
    assert_eq!(
        invalidation.damage().rects(),
        &[Rect::new(
            Point::new(px(10), px(5)),
            Size::new(px(20), px(10)),
        ),],
    );
}
