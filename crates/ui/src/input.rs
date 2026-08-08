use crate::{FrameArena, ListenerId, NodeId, Point, element_state::ElementStateId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ClickTarget {
    pub(crate) node: NodeId,
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
            let node = self.node(node_id);
            if node.layout.bounds.contains(position)
                && let (Some(element), Some(listener)) =
                    (node.element_state_id, node.interaction.click)
            {
                hit = Some(ClickTarget {
                    node: node_id,
                    element,
                    listener,
                })
            }

            current = self.next_depth_first_node(node_id);
        }

        hit
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
        assert!(!runtime.pointer_up(Point::new(px(150), px(80),)).unwrap());
        assert_eq!(clicks.get(), 0);
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
}
