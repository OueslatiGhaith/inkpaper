use crate::{FrameArena, Invalidation, ListenerId, NodeId, Point, element_state::ElementStateId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ClickTarget {
    pub(crate) element: ElementStateId,
    pub(crate) listener: ListenerId,
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct PointerState {
    pressed: Option<ElementStateId>,
}

impl PointerState {
    pub(crate) fn press(&mut self, element: Option<ElementStateId>) {
        self.pressed = element;
    }

    pub(crate) fn pressed(&self) -> Option<ElementStateId> {
        self.pressed
    }

    pub(crate) fn take_pressed(&mut self) -> Option<ElementStateId> {
        self.pressed.take()
    }

    pub(crate) fn cancel(&mut self) {
        self.pressed = None;
    }
}

impl<const NODES: usize, const TEXT_BYTES: usize> FrameArena<NODES, TEXT_BYTES> {
    pub(crate) fn hit_test_click(&self, root: NodeId, position: Point) -> Option<ClickTarget> {
        let mut current = Some(root);
        let mut hit = None;

        while let Some(node_id) = current {
            if self.visual_bounds(node_id).contains(position)
                && self.point_visible_for_node(node_id, position)
                && let Some(target) = self.click_target_from_node(node_id)
            {
                hit = Some(target)
            }

            current = self.next_depth_first_node(node_id);
        }

        hit
    }

    fn click_target_from_node(&self, node_id: NodeId) -> Option<ClickTarget> {
        let node = self.node(node_id);
        let element = node.element_state_id?;
        let listener = node.interaction.click?;

        Some(ClickTarget { element, listener })
    }

    pub(crate) fn click_target_for_element(
        &self,
        root: NodeId,
        element: ElementStateId,
    ) -> Option<ClickTarget> {
        let mut current = Some(root);
        while let Some(node_id) = current {
            if let Some(target) = self.click_target_from_node(node_id)
                && target.element == element
            {
                return Some(target);
            }

            current = self.next_depth_first_node(node_id);
        }

        None
    }

    pub(crate) fn next_click_target(
        &self,
        root: NodeId,
        current_element: Option<ElementStateId>,
    ) -> Option<ClickTarget> {
        let first = self.first_click_target(root)?;
        let Some(current_element) = current_element else {
            return Some(first);
        };

        let mut found_current = false;
        let mut current = Some(root);

        while let Some(node_id) = current {
            if let Some(target) = self.click_target_from_node(node_id) {
                if found_current {
                    return Some(target);
                }
                if target.element == current_element {
                    found_current = true
                }
            }

            current = self.next_depth_first_node(node_id);
        }

        Some(first)
    }

    fn first_click_target(&self, root: NodeId) -> Option<ClickTarget> {
        let mut current = Some(root);

        while let Some(node_id) = current {
            if let Some(target) = self.click_target_from_node(node_id) {
                return Some(target);
            }

            current = self.next_depth_first_node(node_id);
        }

        None
    }

    fn last_click_target(&self, root: NodeId) -> Option<ClickTarget> {
        let mut current = Some(root);
        let mut last = None;

        while let Some(node_id) = current {
            if let Some(target) = self.click_target_from_node(node_id) {
                last = Some(target);
            }

            current = self.next_depth_first_node(node_id);
        }

        last
    }

    pub(crate) fn previous_click_target(
        &self,
        root: NodeId,
        current_element: Option<ElementStateId>,
    ) -> Option<ClickTarget> {
        let last = self.last_click_target(root)?;
        let Some(current_element) = current_element else {
            return Some(last);
        };

        let mut previous = None;
        let mut current = Some(root);

        while let Some(node_id) = current {
            if let Some(target) = self.click_target_from_node(node_id) {
                if target.element == current_element {
                    return previous.or(Some(last));
                }

                previous = Some(target);
            }

            current = self.next_depth_first_node(node_id);
        }

        Some(last)
    }

    pub(crate) fn focused_style_invalidation(&self, element: ElementStateId) -> Invalidation {
        for node in self.nodes.iter() {
            if node.element_state_id == Some(element) {
                return node.interaction.focused_style.invalidation();
            }
        }

        Invalidation::None
    }

    pub(crate) fn pressed_style_invalidation(&self, element: ElementStateId) -> Invalidation {
        for node in self.nodes.iter() {
            if node.element_state_id == Some(element) {
                return node.interaction.pressed_style.invalidation();
            }
        }

        Invalidation::None
    }
}

#[cfg(test)]
mod tests {
    use core::cell::Cell;
    use std::rc::Rc;

    use crate::*;

    type TestRuntime = Runtime<4096, 16, 4096, 32, 64, 512, 32>;

    struct TestTextMeasurer;

    impl TextMeasurer for TestTextMeasurer {
        fn measure(&self, text: &str, max_size: Size) -> Size {
            let width = (text.chars().count() as i32)
                .saturating_mul(6)
                .min(max_size.width.0.max(0));

            let height = if text.is_empty() {
                0
            } else {
                10.min(max_size.height.0.max(0))
            };

            Size::new(px(width), px(height))
        }
    }

    struct Counter {
        clicks: Rc<Cell<u32>>,
    }

    impl Counter {
        fn clicked(&mut self, _: &ClickEvent, cx: &mut Context<Self>) {
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
                    .on_click(cx.listener(Self::clicked))
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

        assert!(runtime.pointer_down(Point::new(px(20), px(20),)));
        assert!(runtime.pointer_up(Point::new(px(20), px(20),)).unwrap());
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

        assert!(runtime.pointer_down(Point::new(px(20), px(20),)));
        assert_eq!(runtime.take_invalidation(), Invalidation::None);
        assert!(!runtime.pointer_up(Point::new(px(150), px(80),)).unwrap());
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

        assert!(!runtime.pointer_down(Point::new(px(150), px(80),)));
        assert!(!runtime.pointer_up(Point::new(px(150), px(80),)).unwrap());
        assert_eq!(clicks.get(), 0);
    }

    struct NestedButtons {
        parent_clicks: Rc<Cell<u32>>,
        child_clicks: Rc<Cell<u32>>,
    }

    impl NestedButtons {
        fn parent_clicked(&mut self, _: &ClickEvent, cx: &mut Context<Self>) {
            self.parent_clicks
                .set(self.parent_clicks.get().saturating_add(1));
            cx.notify();
        }

        fn child_clicked(&mut self, _: &ClickEvent, cx: &mut Context<Self>) {
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
                    .on_click(parent_listener)
                    .child(
                        div()
                            .id("child")
                            .w(px(50))
                            .h(px(50))
                            .on_click(child_listener),
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

        assert!(runtime.pointer_down(Point::new(px(20), px(20),)));
        assert!(runtime.pointer_up(Point::new(px(20), px(20),)).unwrap());
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

        assert!(runtime.pointer_down(Point::new(px(20), px(20),)));

        runtime.rebuild(counter).unwrap();
        runtime
            .layout(Size::new(px(200), px(100)), &TestTextMeasurer)
            .unwrap();

        assert!(runtime.pointer_up(Point::new(px(20), px(20),)).unwrap());
        assert_eq!(clicks.get(), 1);
    }

    struct FocusApp {
        first: Rc<Cell<u32>>,
        second: Rc<Cell<u32>>,
        third: Rc<Cell<u32>>,
    }

    impl FocusApp {
        fn first_clicked(&mut self, _: &ClickEvent, cx: &mut Context<Self>) {
            self.first.set(self.first.get().saturating_add(1));
            cx.notify();
        }

        fn second_clicked(&mut self, _: &ClickEvent, cx: &mut Context<Self>) {
            self.second.set(self.second.get().saturating_add(1));
            cx.notify();
        }

        fn third_clicked(&mut self, _: &ClickEvent, cx: &mut Context<Self>) {
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
                        .on_click(first)
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
                        .on_click(second)
                        .child("Second"),
                )
                .child(
                    div()
                        .id("third")
                        .w(px(80))
                        .h(px(20))
                        .on_click(third)
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

        assert!(runtime.pointer_down(Point::new(px(10), px(55),)));
        assert!(runtime.pointer_up(Point::new(px(10), px(55),)).unwrap());
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
}
