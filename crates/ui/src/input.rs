use core::any::TypeId;

use crate::{
    FrameArena, Invalidation, NodeId, Offset, Pixels, Point, Rect,
    callback::CallbackId,
    element_state::ElementStateId,
    px,
    scroll::{ScrollAxes, ScrollStateTable},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ClickTarget {
    pub(crate) element: ElementStateId,
    pub(crate) listener: CallbackId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FocusTarget {
    pub(crate) element: ElementStateId,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ScrollTarget {
    pub(crate) node: NodeId,
    pub(crate) element: ElementStateId,
    pub(crate) axes: ScrollAxes,
    pub(crate) max_offset: Offset,
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

fn scroll_axis_into_view(
    current: Pixels,
    maximum: Pixels,
    target_start: Pixels,
    target_end: Pixels,
    viewport_start: Pixels,
    viewport_end: Pixels,
) -> Pixels {
    let maximum = maximum.non_negative();
    if viewport_end <= viewport_start {
        return current.clamp(px(0), maximum);
    }

    let delta = if target_start < viewport_start && target_end > viewport_end {
        // the target is larger than the viewport and already spans both edges
        // there is no position that can make it fully visible, so don't jump
        px(0)
    } else if target_start < viewport_start {
        target_start - viewport_start
    } else if target_end > viewport_end {
        target_end - viewport_end
    } else {
        px(0)
    };

    (current + delta).clamp(px(0), maximum)
}

impl<const NODES: usize, const TEXT_BYTES: usize> FrameArena<NODES, TEXT_BYTES> {
    pub(crate) fn hit_test_event(
        &self,
        root: NodeId,
        position: Point,
        event_type: TypeId,
    ) -> Option<NodeId> {
        let mut hit = None;

        for visual in self.visual_nodes(root) {
            if !visual.contains(position) {
                continue;
            }

            let node = visual.node();
            if self.event_callbacks(node, event_type).next().is_some() {
                hit = Some(node);
            }
        }

        hit
    }

    pub(crate) fn hit_test_click(&self, root: NodeId, position: Point) -> Option<ClickTarget> {
        let mut hit = None;

        for visual in self.visual_nodes(root) {
            if !visual.contains(position) {
                continue;
            }
            if let Some(target) = self.click_target_from_node(visual.node()) {
                hit = Some(target)
            }
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

    fn focus_target_from_node(&self, node_id: NodeId) -> Option<FocusTarget> {
        let node = self.node(node_id);
        if !node.interaction.focusable {
            return None;
        }

        let element = node.element_state_id?;

        Some(FocusTarget { element })
    }

    pub(crate) fn focus_target_for_element(
        &self,
        root: NodeId,
        element: ElementStateId,
    ) -> Option<FocusTarget> {
        let mut current = Some(root);

        while let Some(node_id) = current {
            if let Some(target) = self.focus_target_from_node(node_id)
                && target.element == element
            {
                return Some(target);
            }

            current = self.next_depth_first_node(node_id);
        }

        None
    }

    fn first_focus_target(&self, root: NodeId) -> Option<FocusTarget> {
        let mut current = Some(root);

        while let Some(node_id) = current {
            if let Some(target) = self.focus_target_from_node(node_id) {
                return Some(target);
            }

            current = self.next_depth_first_node(node_id);
        }

        None
    }

    fn last_focus_target(&self, root: NodeId) -> Option<FocusTarget> {
        let mut current = Some(root);
        let mut last = None;

        while let Some(node_id) = current {
            if let Some(target) = self.focus_target_from_node(node_id) {
                last = Some(target);
            }

            current = self.next_depth_first_node(node_id);
        }

        last
    }

    pub(crate) fn next_focus_target(
        &self,
        root: NodeId,
        current_element: Option<ElementStateId>,
    ) -> Option<FocusTarget> {
        let first = self.first_focus_target(root)?;

        let Some(current_element) = current_element else {
            return Some(first);
        };

        let mut found_current = false;
        let mut current = Some(root);

        while let Some(node_id) = current {
            if let Some(target) = self.focus_target_from_node(node_id) {
                if found_current {
                    return Some(target);
                }

                if target.element == current_element {
                    found_current = true;
                }
            }

            current = self.next_depth_first_node(node_id);
        }

        Some(first)
    }

    pub(crate) fn previous_focus_target(
        &self,
        root: NodeId,
        current_element: Option<ElementStateId>,
    ) -> Option<FocusTarget> {
        let last = self.last_focus_target(root)?;

        let Some(current_element) = current_element else {
            return Some(last);
        };

        let mut previous = None;
        let mut current = Some(root);

        while let Some(node_id) = current {
            if let Some(target) = self.focus_target_from_node(node_id) {
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

    pub(crate) fn hit_test_scroll(&self, root: NodeId, position: Point) -> Option<ScrollTarget> {
        let mut hit = None;

        for visual in self.visual_nodes(root) {
            if !visual.contains(position) {
                continue;
            }

            let node_id = visual.node();
            let node = self.node(node_id);
            let axes = node.interaction.scroll_axes;
            if !axes.any() {
                continue;
            }
            let Some(element) = node.element_state_id else {
                continue;
            };

            hit = Some(ScrollTarget {
                node: node_id,
                element,
                axes,
                max_offset: self.max_scroll_offset(node_id),
            });
        }

        hit
    }

    pub(crate) fn set_scroll_offset(&mut self, node: NodeId, offset: Offset) {
        self.node_mut(node).interaction.scroll_offset = offset;
    }

    pub(crate) fn clamp_scroll_offset<const SLOTS: usize>(
        &mut self,
        states: &mut ScrollStateTable<SLOTS>,
    ) {
        for index in 0..self.nodes.len() {
            let node_id = NodeId::new(index as u16);
            let node = self.node(node_id);
            let axes = node.interaction.scroll_axes;
            if !axes.any() {
                continue;
            }
            let Some(element) = node.element_state_id else {
                continue;
            };

            let maximum = self.max_scroll_offset(node_id);
            let current = states.offset(element);
            let next = Offset::new(
                if axes.horizontal() {
                    current.x.clamp(px(0), maximum.x)
                } else {
                    px(0)
                },
                if axes.vertical() {
                    current.y.clamp(px(0), maximum.y)
                } else {
                    px(0)
                },
            );

            states.set_offset(element, next);

            self.node_mut(node_id).interaction.scroll_offset = next;
        }
    }

    pub(crate) fn node_for_element(&self, root: NodeId, element: ElementStateId) -> Option<NodeId> {
        let mut current = Some(root);

        while let Some(node_id) = current {
            if self.node(node_id).element_state_id == Some(element) {
                return Some(node_id);
            }

            current = self.next_depth_first_node(node_id);
        }

        None
    }

    fn visual_bounds_for_pair(
        &self,
        root: NodeId,
        first: NodeId,
        second: NodeId,
    ) -> Option<(Rect, Rect)> {
        let mut first_bounds = None;
        let mut second_bounds = None;

        for visual in self.visual_nodes(root) {
            let node = visual.node();
            if node == first {
                first_bounds = Some(visual.bounds());
            }
            if node == second {
                second_bounds = Some(visual.bounds());
            }

            if let (Some(first), Some(second)) = (first_bounds, second_bounds) {
                return Some((first, second));
            }
        }

        None
    }

    fn scroll_viewport_bounds(&self, node: NodeId, visual_bounds: Rect) -> Rect {
        let border = self
            .node(node)
            .style()
            .map(|s| s.border_width.non_negative())
            .unwrap_or_default();

        visual_bounds.inset(border)
    }

    pub(crate) fn scroll_element_into_view<const SLOTS: usize>(
        &mut self,
        root: NodeId,
        element: ElementStateId,
        states: &mut ScrollStateTable<SLOTS>,
    ) -> bool {
        let Some(target_node) = self.node_for_element(root, element) else {
            return false;
        };

        // start with the nearest parent so nested scroll areas are adjusted
        // from inside out
        let mut changed = false;
        let mut current = self.node(target_node).parent;

        while let Some(scroll_node) = current {
            let next_parent = self.node(scroll_node).parent;
            let (axes, scroll_element) = {
                let node = self.node(scroll_node);
                (node.interaction.scroll_axes, node.element_state_id)
            };

            if axes.any()
                && let Some(scroll_element) = scroll_element
                && let Some((target_bounds, scroll_bounds)) =
                    self.visual_bounds_for_pair(root, target_node, scroll_node)
            {
                let viewport = self.scroll_viewport_bounds(scroll_node, scroll_bounds);
                let previous = states.offset(scroll_element);
                let maximum = self.max_scroll_offset(scroll_node);
                let mut next = previous;

                if axes.horizontal() {
                    next.x = scroll_axis_into_view(
                        previous.x,
                        maximum.x,
                        target_bounds.x(),
                        target_bounds.right(),
                        viewport.x(),
                        viewport.right(),
                    );
                }
                if axes.vertical() {
                    next.y = scroll_axis_into_view(
                        previous.y,
                        maximum.y,
                        target_bounds.y(),
                        target_bounds.bottom(),
                        viewport.y(),
                        viewport.bottom(),
                    );
                }
                if next != previous {
                    states.set_offset(scroll_element, next);
                    self.set_scroll_offset(scroll_node, next);
                    changed = true;
                }
            }

            current = next_parent;
        }

        changed
    }
}

#[cfg(test)]
mod tests {
    use core::{any::TypeId, cell::Cell};
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

    struct FocusScrollApp;
    impl FocusScrollApp {
        fn clicked(&mut self, _: &ClickEvent, _: &mut Context<Self>) {}
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
                        .on_click(cx.listener(Self::clicked))
                        .child("First"),
                )
                .child(
                    div()
                        .id("second")
                        .w(px(100))
                        .h(px(30))
                        .on_click(cx.listener(Self::clicked))
                        .child("Second"),
                )
                .child(
                    div()
                        .id("third")
                        .w(px(100))
                        .h(px(30))
                        .on_click(cx.listener(Self::clicked))
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
            Offset::ZERO
        );

        runtime.take_invalidation();

        assert!(runtime.focus_next());
        assert_eq!(
            runtime.frame().node(scroll).interaction.scroll_offset,
            Offset::new(px(0), px(20),)
        );
        assert_eq!(runtime.take_invalidation(), Invalidation::Paint);
        assert!(runtime.focus_next());
        assert_eq!(
            runtime.frame().node(scroll).interaction.scroll_offset,
            Offset::new(px(0), px(50),)
        );

        runtime.take_invalidation();

        assert!(runtime.focus_previous());
        assert_eq!(
            runtime.frame().node(scroll).interaction.scroll_offset,
            Offset::new(px(0), px(30),)
        );
    }

    struct NestedFocusScrollApp;
    impl NestedFocusScrollApp {
        fn clicked(&mut self, _: &ClickEvent, _: &mut Context<Self>) {}
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
                                .on_click(cx.listener(Self::clicked))
                                .child("First"),
                        )
                        .child(
                            div()
                                .id("second")
                                .w(px(100))
                                .h(px(30))
                                .on_click(cx.listener(Self::clicked))
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
        fn clicked(&mut self, _: &ClickEvent, _: &mut Context<Self>) {}
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
                        .on_click(cx.listener(Self::clicked))
                        .child("First"),
                )
                .child(
                    div()
                        .id("second")
                        .w(px(100))
                        .h(px(10))
                        .when_focused(|style| style.h(px(30)))
                        .on_click(cx.listener(Self::clicked))
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
        fn clicked(&mut self, _: &ClickEvent, _: &mut Context<Self>) {}
    }

    impl Render for ClickRemainsFocusableApp {
        fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
            div().child(
                div()
                    .id("button")
                    .w(px(40))
                    .h(px(20))
                    .on_click(cx.listener(Self::clicked)),
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
}
