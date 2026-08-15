use crate::{Point, Rect, Size};

const DAMAGE_RECT_CAPACITY: usize = 4;
const EMPTY_DAMAGE_RECT: Rect = Rect::new(Point::ZERO, Size::ZERO);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DamageRegion {
    rects: [Rect; DAMAGE_RECT_CAPACITY],
    len: u8,
    full: bool,
}

impl Default for DamageRegion {
    fn default() -> Self {
        Self::none()
    }
}

impl DamageRegion {
    pub const fn none() -> Self {
        Self {
            rects: [EMPTY_DAMAGE_RECT; DAMAGE_RECT_CAPACITY],
            len: 0,
            full: false,
        }
    }

    pub const fn full() -> Self {
        Self {
            rects: [EMPTY_DAMAGE_RECT; DAMAGE_RECT_CAPACITY],
            len: 0,
            full: true,
        }
    }

    pub fn from_rect(rect: Rect) -> Self {
        Self::none().add_rect(rect)
    }

    pub const fn is_none(self) -> bool {
        !self.full && self.len == 0
    }

    pub const fn is_full(self) -> bool {
        self.full
    }

    pub const fn len(self) -> usize {
        self.len as usize
    }

    pub fn rects(&self) -> &[Rect] {
        &self.rects[..self.len as usize]
    }

    pub fn add_rect(mut self, rect: Rect) -> Self {
        if self.full || rect.width().is_non_positive() || rect.height().is_non_positive() {
            return self;
        }

        let mut candidate = rect;
        let mut index = 0usize;

        while index < self.len as usize {
            let existing = self.rects[index];

            if damage_rects_touch_or_overlap(candidate, existing) {
                candidate = damage_rect_union(candidate, existing);

                self.remove_rect(index);

                /*
                 * The enlarged candidate can now overlap
                 * a rectangle we checked earlier, so
                 * restart the small bounded scan.
                 */
                index = 0;
            } else {
                index += 1;
            }
        }

        if (self.len as usize) < DAMAGE_RECT_CAPACITY {
            self.rects[self.len as usize] = candidate;

            self.len += 1;

            return self;
        }

        /*
         * Damage storage is deliberately bounded.
         *
         * If it fills up, correctness wins over
         * precision: collapse all rectangles into one
         * conservative bounding rectangle.
         */
        let mut collapsed = candidate;

        for index in 0..self.len as usize {
            collapsed = damage_rect_union(collapsed, self.rects[index]);
        }

        self.rects[0] = collapsed;
        self.len = 1;

        self
    }

    pub fn merge(mut self, other: Self) -> Self {
        if self.full || other.full {
            return Self::full();
        }

        for rect in other.rects {
            self = self.add_rect(rect);
        }

        self
    }

    fn remove_rect(&mut self, index: usize) {
        let len = self.len as usize;
        debug_assert!(index < len);

        for cursor in index..len - 1 {
            self.rects[cursor] = self.rects[cursor + 1]
        }

        self.len -= 1;
        self.rects[self.len as usize] = EMPTY_DAMAGE_RECT;
    }
}

fn damage_rect_union(first: Rect, second: Rect) -> Rect {
    let left = first.x().min(second.x());
    let top = first.y().min(second.y());
    let right = first.right().max(second.right());
    let bottom = first.bottom().max(second.bottom());

    Rect::new(Point::new(left, top), Size::new(right - left, bottom - top))
}

fn damage_rects_touch_or_overlap(first: Rect, second: Rect) -> bool {
    !(first.right() < second.x()
        || second.right() < first.x()
        || first.bottom() < second.y()
        || second.bottom() < first.y())
}

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

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RenderInvalidation {
    kind: Invalidation,
    damage: DamageRegion,
}

impl RenderInvalidation {
    pub const fn none() -> Self {
        Self {
            kind: Invalidation::None,
            damage: DamageRegion::none(),
        }
    }

    pub const fn full(kind: Invalidation) -> Self {
        match kind {
            Invalidation::None => Self::none(),
            _ => Self {
                kind,
                damage: DamageRegion::full(),
            },
        }
    }

    pub fn damaged(kind: Invalidation, damage: DamageRegion) -> Self {
        if matches!(kind, Invalidation::None) {
            return Self::none();
        }

        Self { kind, damage }
    }

    pub const fn kind(self) -> Invalidation {
        self.kind
    }

    pub const fn damage(self) -> DamageRegion {
        self.damage
    }

    pub const fn is_none(self) -> bool {
        matches!(self.kind, Invalidation::None)
    }

    pub fn merge(self, other: Self) -> Self {
        Self {
            kind: self.kind.merge(other.kind),
            damage: self.damage.merge(other.damage),
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

    struct PaintFocusApp;
    impl PaintFocusApp {
        fn clicked(&mut self, _: &ActivateEvent, _cx: &mut Context<Self>) {}
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
                        .on_activate(cx.listener(Self::clicked)),
                )
                .child(
                    div()
                        .id("second")
                        .w(px(60))
                        .h(px(30))
                        .bg(Color::BLUE)
                        .when_focused(|style| style.bg(Color::RED))
                        .on_activate(cx.listener(Self::clicked)),
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
        fn clicked(&mut self, _: &ActivateEvent, _cx: &mut Context<Self>) {}
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
                        .on_activate(cx.listener(Self::clicked)),
                )
                .child(
                    div()
                        .id("second")
                        .w(px(60))
                        .h(px(30))
                        .when_focused(|style| style.bg(Color::GREEN))
                        .on_activate(cx.listener(Self::clicked)),
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
        fn clicked(&mut self, _: &ActivateEvent, _cx: &mut Context<Self>) {}
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
                    .on_activate(cx.listener(Self::clicked)),
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

        assert!(runtime.begin_activation_at(Point::new(px(10), px(10),)));
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

        assert!(runtime.begin_activation_at(Point::new(px(10), px(10),)));

        runtime.take_invalidation();

        assert!(
            runtime
                .complete_activation_at(Point::new(px(10), px(10),))
                .unwrap()
        );
        assert_eq!(runtime.take_invalidation(), Invalidation::Paint);
    }

    struct LayoutPressApp;

    impl LayoutPressApp {
        fn clicked(&mut self, _: &ActivateEvent, _cx: &mut Context<Self>) {}
    }

    impl Render for LayoutPressApp {
        fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
            div().w_full().h_full().child(
                div()
                    .id("button")
                    .w(px(80))
                    .h(px(30))
                    .when_pressed(|style| style.w(px(120)).h(px(40)))
                    .on_activate(cx.listener(Self::clicked)),
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

        assert!(runtime.begin_activation_at(Point::new(px(10), px(10),)));
        assert_eq!(runtime.take_invalidation(), Invalidation::Layout);
    }

    struct NoVisualFocusApp;

    impl NoVisualFocusApp {
        fn clicked(&mut self, _: &ActivateEvent, _cx: &mut Context<Self>) {}
    }

    impl Render for NoVisualFocusApp {
        fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
            div().child(
                div()
                    .id("button")
                    .w(px(80))
                    .h(px(30))
                    .on_activate(cx.listener(Self::clicked)),
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
        fn clicked(&mut self, _: &ActivateEvent, cx: &mut Context<Self>) {
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
                    .on_activate(cx.listener(Self::clicked)),
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

        assert!(runtime.begin_activation_at(Point::new(px(10), px(10),)));
        assert_eq!(runtime.take_invalidation(), Invalidation::Paint);
        assert!(
            runtime
                .complete_activation_at(Point::new(px(10), px(10),))
                .unwrap()
        );
        assert_eq!(clicks.get(), 1);
        assert_eq!(runtime.take_invalidation(), Invalidation::Rebuild);
    }

    #[test]
    fn focused_text_color_change_requires_paint_only() {
        struct App;
        impl App {
            fn clicked(&mut self, _: &ActivateEvent, _: &mut Context<Self>) {}
        }

        impl Render for App {
            fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
                div().child(
                    div()
                        .id("button")
                        .text_color(Color::WHITE)
                        .when_focused(|style| style.text_color(Color::RED))
                        .on_activate(cx.listener(Self::clicked))
                        .child("Button"),
                )
            }
        }

        let mut runtime = TestRuntime::default();

        let app = runtime.create(|_| App).unwrap();

        runtime.rebuild(app).unwrap();
        runtime
            .layout(Size::new(px(200), px(100)), &TestTextMeasurer)
            .unwrap();

        assert!(runtime.focus_next());
        assert_eq!(runtime.take_invalidation(), Invalidation::Paint);
    }

    #[test]
    fn focused_font_change_requires_layout() {
        struct App;
        impl App {
            fn clicked(&mut self, _: &ActivateEvent, _: &mut Context<Self>) {}
        }

        impl Render for App {
            fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
                div().child(
                    div()
                        .id("button")
                        .font(FontId::new(0))
                        .when_focused(|style| style.font(FontId::new(1)))
                        .on_activate(cx.listener(Self::clicked))
                        .child("Button"),
                )
            }
        }

        let mut runtime = TestRuntime::default();

        let app = runtime.create(|_| App).unwrap();

        runtime.rebuild(app).unwrap();
        runtime
            .layout(Size::new(px(200), px(100)), &TestTextMeasurer)
            .unwrap();

        assert!(runtime.focus_next());
        assert_eq!(runtime.take_invalidation(), Invalidation::Layout);
    }

    #[test]
    fn focused_text_alignment_change_requires_paint_only() {
        struct App;
        impl App {
            fn clicked(&mut self, _: &ActivateEvent, _: &mut Context<Self>) {}
        }

        impl Render for App {
            fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
                div().child(
                    div()
                        .id("button")
                        .text_start()
                        .when_focused(|style| style.text_center())
                        .on_activate(cx.listener(Self::clicked))
                        .child("Button"),
                )
            }
        }

        let mut runtime = TestRuntime::default();

        let app = runtime.create(|_| App).unwrap();

        runtime.rebuild(app).unwrap();
        runtime
            .layout(Size::new(px(200), px(100)), &TestTextMeasurer)
            .unwrap();

        assert!(runtime.focus_next());
        assert_eq!(runtime.take_invalidation(), Invalidation::Paint);
    }

    #[test]
    fn focused_text_wrap_change_requires_layout() {
        struct App;
        impl App {
            fn clicked(&mut self, _: &ActivateEvent, _: &mut Context<Self>) {}
        }

        impl Render for App {
            fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
                div().child(
                    div()
                        .id("button")
                        .no_wrap()
                        .when_focused(|style| style.wrap())
                        .on_activate(cx.listener(Self::clicked))
                        .child("A long button label"),
                )
            }
        }

        let mut runtime = TestRuntime::default();

        let app = runtime.create(|_| App).unwrap();

        runtime.rebuild(app).unwrap();
        runtime
            .layout(Size::new(px(200), px(100)), &TestTextMeasurer)
            .unwrap();

        assert!(runtime.focus_next());
        assert_eq!(runtime.take_invalidation(), Invalidation::Layout);
    }

    #[test]
    fn max_lines_change_requires_layout() {
        let base = Style::default();

        let mut variant = base;
        variant.text.max_lines = Some(TextMaxLines::Limited(2));

        let patch = StylePatch::between(base, variant);

        assert_eq!(patch.invalidation(), Invalidation::Layout);
    }

    #[test]
    fn text_overflow_change_requires_layout() {
        let base = Style::default();

        let mut variant = base;
        variant.text.overflow = Some(TextOverflow::Ellipsis);

        let patch = StylePatch::between(base, variant);

        assert_eq!(patch.invalidation(), Invalidation::Layout);
    }

    #[test]
    fn damage_region_keeps_disjoint_rectangles() {
        let first = Rect::new(Point::new(px(0), px(0)), Size::new(px(10), px(10)));
        let second = Rect::new(Point::new(px(30), px(30)), Size::new(px(10), px(10)));

        let damage = DamageRegion::none().add_rect(first).add_rect(second);

        assert!(!damage.is_full());
        assert_eq!(damage.len(), 2);
        assert_eq!(damage.rects(), &[first, second],);
    }

    #[test]
    fn damage_region_coalesces_touching_rectangles() {
        let first = Rect::new(Point::new(px(10), px(10)), Size::new(px(20), px(20)));
        let second = Rect::new(Point::new(px(30), px(10)), Size::new(px(20), px(20)));

        let damage = DamageRegion::none().add_rect(first).add_rect(second);

        assert_eq!(damage.len(), 1);
        assert_eq!(
            damage.rects(),
            &[Rect::new(
                Point::new(px(10), px(10),),
                Size::new(px(40), px(20),),
            ),],
        );
    }

    #[test]
    fn damage_region_collapses_when_capacity_is_exceeded() {
        let damage = DamageRegion::none()
            .add_rect(Rect::new(Point::new(px(0), px(0)), Size::new(px(2), px(2))))
            .add_rect(Rect::new(
                Point::new(px(10), px(0)),
                Size::new(px(2), px(2)),
            ))
            .add_rect(Rect::new(
                Point::new(px(20), px(0)),
                Size::new(px(2), px(2)),
            ))
            .add_rect(Rect::new(
                Point::new(px(30), px(0)),
                Size::new(px(2), px(2)),
            ))
            .add_rect(Rect::new(
                Point::new(px(40), px(0)),
                Size::new(px(2), px(2)),
            ));

        assert!(!damage.is_full());
        assert_eq!(damage.len(), 1);
        assert_eq!(
            damage.rects(),
            &[Rect::new(
                Point::new(px(0), px(0),),
                Size::new(px(42), px(2),),
            ),],
        );
    }

    #[test]
    fn full_damage_absorbs_partial_damage() {
        let partial = DamageRegion::from_rect(Rect::new(
            Point::new(px(10), px(10)),
            Size::new(px(20), px(20)),
        ));

        assert!(partial.merge(DamageRegion::full(),).is_full());
        assert!(DamageRegion::full().merge(partial).is_full());
    }

    #[test]
    fn paint_invalidation_carries_damage() {
        let mut runtime = TestRuntime::default();

        let app = runtime.create(|_| PaintFocusApp).unwrap();

        runtime.rebuild(app).unwrap();
        runtime
            .layout(Size::new(px(200), px(100)), &TestTextMeasurer)
            .unwrap();

        assert!(runtime.focus_next());

        let invalidation = runtime.take_render_invalidation();

        assert_eq!(invalidation.kind(), Invalidation::Paint,);
        assert!(invalidation.damage().is_full());
        assert_eq!(runtime.invalidation(), Invalidation::None,);
        assert!(runtime.damage().is_none());
    }
}
