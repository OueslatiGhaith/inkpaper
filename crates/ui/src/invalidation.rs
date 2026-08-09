#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Invalidation {
    #[default]
    None,
    Paint,
    Layout,
    Rebuild,
}

impl Invalidation {
    pub const fn merge(self, other: Self) -> Self {
        use Invalidation::*;

        match (self, other) {
            (Rebuild, _) | (_, Rebuild) => Rebuild,
            (Layout, _) | (_, Layout) => Layout,
            (Paint, _) | (_, Paint) => Paint,
            _ => None,
        }
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

    struct PaintFocusApp;
    impl PaintFocusApp {
        fn clicked(&mut self, _: &ClickEvent, _cx: &mut Context<Self>) {}
    }

    impl Render for PaintFocusApp {
        fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
            div()
                .child(
                    div()
                        .id("first")
                        .w(px(60))
                        .h(px(30))
                        .bg(Color::BLUE)
                        .when_focused(|style| style.bg(Color::GREEN))
                        .on_click(cx.listener(Self::clicked)),
                )
                .child(
                    div()
                        .id("second")
                        .w(px(60))
                        .h(px(30))
                        .bg(Color::BLUE)
                        .when_focused(|style| style.bg(Color::RED))
                        .on_click(cx.listener(Self::clicked)),
                )
        }
    }

    #[test]
    fn color_only_focus_change_requires_paint_only() {
        let mut runtime = TestRuntime::default();

        let app = runtime.create(|_| PaintFocusApp).unwrap();

        runtime.rebuild(app).unwrap();
        runtime
            .layout(Size::new(px(200), px(100)), &TestTextMeasurer)
            .unwrap();

        assert!(runtime.focus_next());
        assert_eq!(runtime.take_invalidation(), Invalidation::Paint);
    }

    #[test]
    fn moving_between_color_only_focus_styles_requires_paint_only() {
        let mut runtime = TestRuntime::default();

        let app = runtime.create(|_| PaintFocusApp).unwrap();

        runtime.rebuild(app).unwrap();
        runtime
            .layout(Size::new(px(200), px(100)), &TestTextMeasurer)
            .unwrap();

        assert!(runtime.focus_next());

        runtime.take_invalidation();

        assert!(runtime.focus_next());
        assert_eq!(runtime.take_invalidation(), Invalidation::Paint);
    }

    struct LayoutFocusApp;

    impl LayoutFocusApp {
        fn clicked(&mut self, _: &ClickEvent, _cx: &mut Context<Self>) {}
    }

    impl Render for LayoutFocusApp {
        fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
            div()
                .flex()
                .w(px(200))
                .child(
                    div()
                        .id("first")
                        .w(px(60))
                        .h(px(30))
                        .when_focused(|style| style.w(px(100)))
                        .on_click(cx.listener(Self::clicked)),
                )
                .child(
                    div()
                        .id("second")
                        .w(px(60))
                        .h(px(30))
                        .when_focused(|style| style.bg(Color::GREEN))
                        .on_click(cx.listener(Self::clicked)),
                )
        }
    }

    #[test]
    fn size_changing_focus_requires_layout() {
        let mut runtime = TestRuntime::default();

        let app = runtime.create(|_| LayoutFocusApp).unwrap();

        runtime.rebuild(app).unwrap();
        runtime
            .layout(Size::new(px(200), px(100)), &TestTextMeasurer)
            .unwrap();

        assert!(runtime.focus_next());
        assert_eq!(runtime.take_invalidation(), Invalidation::Layout);
    }

    #[test]
    fn removing_layout_affecting_focus_still_requires_layout() {
        let mut runtime = TestRuntime::default();

        let app = runtime.create(|_| LayoutFocusApp).unwrap();

        runtime.rebuild(app).unwrap();
        runtime
            .layout(Size::new(px(200), px(100)), &TestTextMeasurer)
            .unwrap();

        assert!(runtime.focus_next());

        runtime.take_invalidation();

        assert!(runtime.focus_next());
        assert_eq!(runtime.take_invalidation(), Invalidation::Layout);
    }

    struct PaintPressApp;

    impl PaintPressApp {
        fn clicked(&mut self, _: &ClickEvent, _cx: &mut Context<Self>) {}
    }

    impl Render for PaintPressApp {
        fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
            div().w_full().h_full().child(
                div()
                    .id("button")
                    .w(px(80))
                    .h(px(30))
                    .bg(Color::BLUE)
                    .when_pressed(|style| style.bg(Color::RED))
                    .on_click(cx.listener(Self::clicked)),
            )
        }
    }

    #[test]
    fn color_only_pressed_style_requires_paint_only() {
        let mut runtime = TestRuntime::default();

        let app = runtime.create(|_| PaintPressApp).unwrap();

        runtime.rebuild(app).unwrap();
        runtime
            .layout(Size::new(px(200), px(100)), &TestTextMeasurer)
            .unwrap();

        assert!(runtime.pointer_down(Point::new(px(10), px(10),)));
        assert_eq!(runtime.take_invalidation(), Invalidation::Paint);
    }

    #[test]
    fn releasing_color_only_pressed_style_requires_paint_only_without_notify() {
        let mut runtime = TestRuntime::default();

        let app = runtime.create(|_| PaintPressApp).unwrap();

        runtime.rebuild(app).unwrap();
        runtime
            .layout(Size::new(px(200), px(100)), &TestTextMeasurer)
            .unwrap();

        assert!(runtime.pointer_down(Point::new(px(10), px(10),)));

        runtime.take_invalidation();

        assert!(runtime.pointer_up(Point::new(px(10), px(10),)).unwrap());
        assert_eq!(runtime.take_invalidation(), Invalidation::Paint);
    }

    struct LayoutPressApp;

    impl LayoutPressApp {
        fn clicked(&mut self, _: &ClickEvent, _cx: &mut Context<Self>) {}
    }

    impl Render for LayoutPressApp {
        fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
            div().w_full().h_full().child(
                div()
                    .id("button")
                    .w(px(80))
                    .h(px(30))
                    .when_pressed(|style| style.w(px(120)).h(px(40)))
                    .on_click(cx.listener(Self::clicked)),
            )
        }
    }

    #[test]
    fn size_changing_pressed_style_requires_layout() {
        let mut runtime = TestRuntime::default();

        let app = runtime.create(|_| LayoutPressApp).unwrap();

        runtime.rebuild(app).unwrap();
        runtime
            .layout(Size::new(px(200), px(100)), &TestTextMeasurer)
            .unwrap();

        assert!(runtime.pointer_down(Point::new(px(10), px(10),)));
        assert_eq!(runtime.take_invalidation(), Invalidation::Layout);
    }

    struct NoVisualFocusApp;

    impl NoVisualFocusApp {
        fn clicked(&mut self, _: &ClickEvent, _cx: &mut Context<Self>) {}
    }

    impl Render for NoVisualFocusApp {
        fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
            div().child(
                div()
                    .id("button")
                    .w(px(80))
                    .h(px(30))
                    .on_click(cx.listener(Self::clicked)),
            )
        }
    }

    #[test]
    fn focus_change_without_interaction_style_requires_no_visual_work() {
        let mut runtime = TestRuntime::default();

        let app = runtime.create(|_| NoVisualFocusApp).unwrap();

        runtime.rebuild(app).unwrap();
        runtime
            .layout(Size::new(px(200), px(100)), &TestTextMeasurer)
            .unwrap();

        assert!(runtime.focus_next());
        assert_eq!(runtime.take_invalidation(), Invalidation::None);
    }

    struct NotifyApp {
        clicks: Rc<Cell<u32>>,
    }

    impl NotifyApp {
        fn clicked(&mut self, _: &ClickEvent, cx: &mut Context<Self>) {
            self.clicks.set(self.clicks.get().saturating_add(1));

            cx.notify();
        }
    }

    impl Render for NotifyApp {
        fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
            div().child(
                div()
                    .id("button")
                    .w(px(80))
                    .h(px(30))
                    .bg(Color::BLUE)
                    .when_pressed(|style| style.bg(Color::RED))
                    .on_click(cx.listener(Self::clicked)),
            )
        }
    }

    #[test]
    fn application_notify_overrides_paint_invalidation_with_rebuild() {
        let clicks = Rc::new(Cell::new(0));
        let mut runtime = TestRuntime::default();

        let app = runtime
            .create({
                let clicks = clicks.clone();

                move |_| NotifyApp { clicks }
            })
            .unwrap();

        runtime.rebuild(app).unwrap();
        runtime
            .layout(Size::new(px(200), px(100)), &TestTextMeasurer)
            .unwrap();

        assert!(runtime.pointer_down(Point::new(px(10), px(10),)));
        assert_eq!(runtime.take_invalidation(), Invalidation::Paint);
        assert!(runtime.pointer_up(Point::new(px(10), px(10),)).unwrap());
        assert_eq!(clicks.get(), 1);
        assert_eq!(runtime.take_invalidation(), Invalidation::Rebuild);
    }
}
