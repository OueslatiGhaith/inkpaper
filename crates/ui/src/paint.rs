use embedded_graphics::{
    Drawable,
    draw_target::{DrawTarget, DrawTargetExt},
    geometry::{Point as EgPoint, Size as EgSize},
    mono_font::{MonoFont, MonoTextStyle},
    pixelcolor::Rgb888,
    primitives::{
        Primitive, PrimitiveStyle, PrimitiveStyleBuilder, Rectangle, RoundedRectangle,
        StrokeAlignment,
    },
    text::{Baseline, Text},
};

use crate::{Color, FrameArena, NodeId, Point, Rect, Size, TextMeasurer, px};

fn to_rgb888(color: Color) -> Rgb888 {
    Rgb888::new(color.r, color.g, color.b)
}

fn to_embedded_rect(rect: Rect) -> Rectangle {
    let width = u32::try_from(rect.size.width.0.max(0)).unwrap_or(u32::MAX);
    let height = u32::try_from(rect.size.height.0.max(0)).unwrap_or(u32::MAX);

    Rectangle::new(
        EgPoint::new(rect.origin.x.0, rect.origin.y.0),
        EgSize::new(width, height),
    )
}

pub trait TextPainter: TextMeasurer {
    fn draw<D>(&self, text: &str, position: Point, target: &mut D) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Rgb888>;
}

pub struct MonoTextPainter<'a> {
    font: &'a MonoFont<'a>,
    color: Color,
}

impl<'a> MonoTextPainter<'a> {
    pub const fn new(font: &'a MonoFont<'a>, color: Color) -> Self {
        Self { font, color }
    }
}

impl TextMeasurer for MonoTextPainter<'_> {
    fn measure(&self, text: &str, max_size: Size) -> Size {
        if text.is_empty() {
            return Size::ZERO;
        }

        let mut longest_line_chars = 0;
        let mut line_count = 0;

        for line in text.split('\n') {
            longest_line_chars = longest_line_chars.max(line.chars().count());
            line_count += 1;
        }

        let character_width = self.font.character_size.width;
        let character_height = self.font.character_size.height;
        let spacing = self.font.character_spacing;

        let characters = u32::try_from(longest_line_chars).unwrap_or(u32::MAX);
        let lines = u32::try_from(line_count).unwrap_or(u32::MAX);

        let width = if characters == 0 {
            0
        } else {
            characters
                .saturating_mul(character_width.saturating_add(spacing))
                .saturating_sub(spacing)
        };
        let height = lines.saturating_mul(character_height);

        let width = i32::try_from(width)
            .unwrap_or(i32::MAX)
            .min(max_size.width.0.max(0));
        let height = i32::try_from(height)
            .unwrap_or(i32::MAX)
            .min(max_size.height.0.max(0));

        Size::new(px(width), px(height))
    }
}

impl TextPainter for MonoTextPainter<'_> {
    fn draw<D>(&self, text: &str, position: Point, target: &mut D) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Rgb888>,
    {
        let style = MonoTextStyle::new(self.font, to_rgb888(self.color));

        Text::with_baseline(
            text,
            EgPoint::new(position.x.0, position.y.0),
            style,
            Baseline::Top,
        )
        .draw(target)
        .map(|_| ())
    }
}

impl<const NODES: usize, const TEXT_BYTES: usize> FrameArena<NODES, TEXT_BYTES> {
    fn next_paint_node(&self, current: NodeId) -> Option<NodeId> {
        if let Some(child) = self.node(current).first_child {
            return Some(child);
        }

        let mut node = current;

        loop {
            if let Some(sibling) = self.node(node).next_sibling {
                return Some(sibling);
            }

            let parent = self.node(node).parent?;
            node = parent
        }
    }

    fn paint_node<D, P>(
        &self,
        node: NodeId,
        target: &mut D,
        text_painter: &P,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Rgb888>,
        P: TextPainter,
    {
        let node = self.node(node);
        match node.kind {
            crate::NodeKind::Div { .. } => {
                let style = node.style().expect("div node must have style");
                let bounds = node.layout.bounds;
                if bounds.width().0 <= 0 || bounds.height().0 <= 0 {
                    return Ok(());
                }

                let border_width = u32::try_from(style.border_width.0.max(0)).unwrap_or(u32::MAX);

                let mut primitive_style =
                    PrimitiveStyleBuilder::new().stroke_alignment(StrokeAlignment::Inside);

                if let Some(background) = style.background {
                    primitive_style = primitive_style.fill_color(to_rgb888(background));
                }
                if border_width > 0
                    && let Some(border_color) = style.border_color
                {
                    primitive_style = primitive_style
                        .stroke_color(to_rgb888(border_color))
                        .stroke_width(border_width);
                }

                let primitive_style = primitive_style.build();
                let rectangle = to_embedded_rect(bounds);

                let radius = u32::try_from(style.border_radius.0.max(0)).unwrap_or(u32::MAX);
                if radius == 0 {
                    rectangle.into_styled(primitive_style).draw(target)?;
                } else {
                    RoundedRectangle::with_equal_corners(rectangle, EgSize::new_equal(radius))
                        .into_styled(primitive_style)
                        .draw(target)?;
                }

                Ok(())
            }
            crate::NodeKind::Text { text } => {
                text_painter.draw(self.text(text), node.layout.bounds.origin, target)
            }
            crate::NodeKind::Entity { .. } => Ok(()),
        }
    }

    fn paint_rgb888<D, P>(
        &self,
        root: NodeId,
        target: &mut D,
        text_painter: &P,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Rgb888>,
        P: TextPainter,
    {
        let mut current = Some(root);
        while let Some(node) = current {
            self.paint_node(node, target, text_painter)?;
            current = self.next_depth_first_node(node);
        }

        Ok(())
    }

    pub fn paint<D, P>(
        &self,
        root: NodeId,
        target: &mut D,
        text_painter: &P,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget,
        D::Color: From<Rgb888>,
        P: TextPainter,
    {
        let mut converted = target.color_converted::<Rgb888>();
        self.paint_rgb888(root, &mut converted, text_painter)
    }
}

#[cfg(test)]
mod tests {
    use core::cell::Cell;
    use std::rc::Rc;

    use embedded_graphics::{
        geometry::Point as EgPoint,
        mock_display::MockDisplay,
        mono_font::ascii::FONT_6X10,
        pixelcolor::{BinaryColor, Rgb888},
    };

    use crate::*;

    #[test]
    fn div_background_is_painted() {
        let painter = MonoTextPainter::new(&FONT_6X10, Color::WHITE);
        let mut frame = FrameArena::<8, 64>::default();
        let mut display = MockDisplay::<Rgb888>::new();

        let root = frame.mount(div().w(px(4)).h(px(3)).bg(Color::RED)).unwrap();

        frame.layout(root, Size::new(px(8), px(8)), &painter);
        frame.paint(root, &mut display, &painter).unwrap();

        assert_eq!(
            display.get_pixel(EgPoint::new(0, 0)),
            Some(Rgb888::new(255, 0, 0,))
        );
        assert_eq!(
            display.get_pixel(EgPoint::new(3, 2)),
            Some(Rgb888::new(255, 0, 0,))
        );
        assert_eq!(display.get_pixel(EgPoint::new(4, 2)), None);
    }

    #[test]
    fn div_without_background_draws_nothing() {
        let painter = MonoTextPainter::new(&FONT_6X10, Color::WHITE);
        let mut frame = FrameArena::<8, 64>::default();
        let mut display = MockDisplay::<Rgb888>::new();

        let root = frame.mount(div().w(px(4)).h(px(3))).unwrap();

        frame.layout(root, Size::new(px(8), px(8)), &painter);
        frame.paint(root, &mut display, &painter).unwrap();

        assert_eq!(display.get_pixel(EgPoint::new(0, 0)), None);
        assert_eq!(display.affected_area().size.width, 0);
        assert_eq!(display.affected_area().size.height, 0);
    }

    #[test]
    fn text_is_painted_at_layout_position() {
        let painter = MonoTextPainter::new(&FONT_6X10, Color::WHITE);
        let mut frame = FrameArena::<8, 64>::default();
        let mut display = MockDisplay::<Rgb888>::new();

        let root = frame.mount(div().p(px(2)).child("A")).unwrap();

        frame.layout(root, Size::new(px(32), px(32)), &painter);

        let text_node = frame.node(root).first_child.unwrap();

        assert_eq!(
            frame.bounds(text_node),
            Rect::new(Point::new(px(2), px(2)), Size::new(px(6), px(10)),)
        );

        display.set_allow_overdraw(true);
        frame.paint(root, &mut display, &painter).unwrap();
        let affected = display.affected_area();

        assert!(affected.size.width > 0);
        assert!(affected.size.height > 0);
        assert!(affected.top_left.x >= 2);
        assert!(affected.top_left.y >= 2);
    }

    #[test]
    fn parent_background_is_painted_before_child_background() {
        let painter = MonoTextPainter::new(&FONT_6X10, Color::WHITE);
        let mut frame = FrameArena::<8, 64>::default();
        let mut display = MockDisplay::<Rgb888>::new();

        let root = frame
            .mount(
                div()
                    .w(px(8))
                    .h(px(8))
                    .bg(Color::RED)
                    .child(div().w(px(4)).h(px(4)).bg(Color::BLUE)),
            )
            .unwrap();

        frame.layout(root, Size::new(px(8), px(8)), &painter);
        display.set_allow_overdraw(true);
        frame.paint(root, &mut display, &painter).unwrap();

        assert_eq!(
            display.get_pixel(EgPoint::new(0, 0)),
            Some(Rgb888::new(0, 0, 255,))
        );
        assert_eq!(
            display.get_pixel(EgPoint::new(7, 7)),
            Some(Rgb888::new(255, 0, 0,))
        );
    }

    #[test]
    fn renderer_converts_rgb888_to_binary_targets() {
        let painter = MonoTextPainter::new(&FONT_6X10, Color::WHITE);
        let mut frame = FrameArena::<8, 64>::default();
        let mut display = MockDisplay::<BinaryColor>::new();

        let root = frame
            .mount(div().w(px(3)).h(px(3)).bg(Color::WHITE))
            .unwrap();

        frame.layout(root, Size::new(px(8), px(8)), &painter);
        frame.paint(root, &mut display, &painter).unwrap();

        assert_eq!(display.get_pixel(EgPoint::new(1, 1)), Some(BinaryColor::On));
    }

    #[test]
    fn monospace_text_measurement_matches_font_metrics() {
        let painter = MonoTextPainter::new(&FONT_6X10, Color::WHITE);

        let measured = crate::TextMeasurer::measure(&painter, "Hello", Size::new(px(100), px(100)));

        assert_eq!(measured, Size::new(px(30), px(10),));
    }

    #[test]
    fn monospace_text_measurement_supports_multiple_lines() {
        let painter = MonoTextPainter::new(&FONT_6X10, Color::WHITE);

        let measured = crate::TextMeasurer::measure(&painter, "AB\nC", Size::new(px(100), px(100)));

        assert_eq!(measured, Size::new(px(12), px(20),));
    }

    #[test]
    fn monospace_text_measurement_is_constrained_by_available_size() {
        let painter = MonoTextPainter::new(&FONT_6X10, Color::WHITE);

        let measured = crate::TextMeasurer::measure(&painter, "Hello", Size::new(px(12), px(7)));

        assert_eq!(measured, Size::new(px(12), px(7),));
    }

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

    struct FocusSizeApp {
        clicks: Rc<Cell<u32>>,
    }

    impl FocusSizeApp {
        fn clicked(&mut self, _: &ClickEvent, cx: &mut Context<Self>) {
            self.clicks.set(self.clicks.get().saturating_add(1));
            cx.notify();
        }
    }

    impl Render for FocusSizeApp {
        fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
            div()
                .flex()
                .w(px(200))
                .h(px(50))
                .gap(px(5))
                .child(
                    div()
                        .id("first")
                        .w(px(60))
                        .h(px(30))
                        .bg(Color::BLUE)
                        .when_focused(|style| style.w(px(100)).bg(Color::GREEN))
                        .on_click(cx.listener(Self::clicked))
                        .child("First"),
                )
                .child(
                    div()
                        .id("second")
                        .w(px(60))
                        .h(px(30))
                        .on_click(cx.listener(Self::clicked))
                        .child("Second"),
                )
        }
    }

    #[test]
    fn focused_style_can_change_layout_size() {
        let clicks = Rc::new(Cell::new(0));
        let mut runtime = TestRuntime::default();

        let app = runtime
            .create({
                let clicks = clicks.clone();

                move |_| FocusSizeApp { clicks }
            })
            .unwrap();

        let root = runtime.rebuild(app).unwrap();
        runtime
            .layout(Size::new(px(200), px(50)), &TestTextMeasurer)
            .unwrap();

        let app_div = runtime.frame().node(root).first_child.unwrap();
        let first = runtime.frame().node(app_div).first_child.unwrap();
        let second = runtime.frame().node(first).next_sibling.unwrap();

        assert_eq!(runtime.frame().bounds(first).width(), px(60));
        assert_eq!(runtime.frame().bounds(second).x(), px(65));
        assert!(runtime.focus_next());
        assert_eq!(runtime.take_invalidation(), Invalidation::Layout);

        runtime
            .layout(Size::new(px(200), px(50)), &TestTextMeasurer)
            .unwrap();

        assert_eq!(runtime.frame().bounds(first).width(), px(100));
        assert_eq!(runtime.frame().bounds(second).x(), px(105));
    }

    #[test]
    fn moving_focus_restores_previous_element_base_style() {
        let clicks = Rc::new(Cell::new(0));
        let mut runtime = TestRuntime::default();

        let app = runtime
            .create({
                let clicks = clicks.clone();

                move |_| FocusSizeApp { clicks }
            })
            .unwrap();

        let root = runtime.rebuild(app).unwrap();
        runtime
            .layout(Size::new(px(200), px(50)), &TestTextMeasurer)
            .unwrap();

        let app_div = runtime.frame().node(root).first_child.unwrap();
        let first = runtime.frame().node(app_div).first_child.unwrap();

        assert!(runtime.focus_next());

        runtime.take_invalidation();
        runtime
            .layout(Size::new(px(200), px(50)), &TestTextMeasurer)
            .unwrap();

        assert_eq!(runtime.frame().bounds(first).width(), px(100));
        assert!(runtime.focus_next());
        assert_eq!(runtime.take_invalidation(), Invalidation::Layout);

        runtime
            .layout(Size::new(px(200), px(50)), &TestTextMeasurer)
            .unwrap();

        assert_eq!(runtime.frame().bounds(first).width(), px(60));
    }

    struct PressSizeApp {
        clicks: Rc<Cell<u32>>,
    }

    impl PressSizeApp {
        fn clicked(&mut self, _: &ClickEvent, cx: &mut Context<Self>) {
            self.clicks.set(self.clicks.get().saturating_add(1));
            cx.notify();
        }
    }

    impl Render for PressSizeApp {
        fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
            div().w_full().h_full().child(
                div()
                    .id("button")
                    .w(px(80))
                    .h(px(30))
                    .when_pressed(|style| style.w(px(110)).h(px(40)))
                    .on_click(cx.listener(Self::clicked))
                    .child("Press"),
            )
        }
    }

    #[test]
    fn pressed_style_can_change_layout_size() {
        let clicks = Rc::new(Cell::new(0));
        let mut runtime = TestRuntime::default();

        let app = runtime
            .create({
                let clicks = clicks.clone();

                move |_| PressSizeApp { clicks }
            })
            .unwrap();

        let root = runtime.rebuild(app).unwrap();
        runtime
            .layout(Size::new(px(200), px(100)), &TestTextMeasurer)
            .unwrap();

        let app_div = runtime.frame().node(root).first_child.unwrap();
        let button = runtime.frame().node(app_div).first_child.unwrap();

        assert_eq!(runtime.frame().bounds(button).width(), px(80));
        assert_eq!(runtime.frame().bounds(button).height(), px(30));
        assert!(runtime.pointer_down(Point::new(px(10), px(10),)));
        assert_eq!(runtime.take_invalidation(), Invalidation::Layout);

        runtime
            .layout(Size::new(px(200), px(100)), &TestTextMeasurer)
            .unwrap();

        assert_eq!(runtime.frame().bounds(button).width(), px(110));
        assert_eq!(runtime.frame().bounds(button).height(), px(40));
    }

    #[test]
    fn releasing_pointer_restores_base_size_and_requests_rebuild_after_click() {
        let clicks = Rc::new(Cell::new(0));
        let mut runtime = TestRuntime::default();

        let app = runtime
            .create({
                let clicks = clicks.clone();

                move |_| PressSizeApp { clicks }
            })
            .unwrap();

        let root = runtime.rebuild(app).unwrap();
        runtime
            .layout(Size::new(px(200), px(100)), &TestTextMeasurer)
            .unwrap();

        let app_div = runtime.frame().node(root).first_child.unwrap();
        let button = runtime.frame().node(app_div).first_child.unwrap();

        assert!(runtime.pointer_down(Point::new(px(10), px(10),)));

        runtime.take_invalidation();
        runtime
            .layout(Size::new(px(200), px(100)), &TestTextMeasurer)
            .unwrap();

        assert_eq!(runtime.frame().bounds(button).width(), px(110));
        assert!(runtime.pointer_up(Point::new(px(10), px(10),)).unwrap());
        assert_eq!(clicks.get(), 1);
        assert_eq!(runtime.take_invalidation(), Invalidation::Rebuild);
    }

    #[test]
    fn pointer_cancel_removes_pressed_style() {
        let clicks = Rc::new(Cell::new(0));
        let mut runtime = TestRuntime::default();

        let app = runtime
            .create({
                let clicks = clicks.clone();

                move |_| PressSizeApp { clicks }
            })
            .unwrap();

        let root = runtime.rebuild(app).unwrap();
        runtime
            .layout(Size::new(px(200), px(100)), &TestTextMeasurer)
            .unwrap();

        let app_div = runtime.frame().node(root).first_child.unwrap();
        let button = runtime.frame().node(app_div).first_child.unwrap();

        assert!(runtime.pointer_down(Point::new(px(10), px(10),)));

        runtime.take_invalidation();
        runtime
            .layout(Size::new(px(200), px(100)), &TestTextMeasurer)
            .unwrap();

        assert_eq!(runtime.frame().bounds(button).width(), px(110));

        runtime.pointer_cancel();

        assert_eq!(runtime.take_invalidation(), Invalidation::Layout);

        runtime
            .layout(Size::new(px(200), px(100)), &TestTextMeasurer)
            .unwrap();

        assert_eq!(runtime.frame().bounds(button).width(), px(80));
    }

    #[test]
    fn border_is_painted_inside_element_bounds() {
        let painter = MonoTextPainter::new(&FONT_6X10, Color::WHITE);
        let mut frame = FrameArena::<8, 64>::default();

        let root = frame
            .mount(
                div()
                    .w(px(10))
                    .h(px(10))
                    .bg(Color::BLUE)
                    .border(px(2))
                    .border_color(Color::RED),
            )
            .unwrap();

        frame.layout(root, Size::new(px(20), px(20)), &painter);

        let mut display = MockDisplay::<Rgb888>::new();
        display.set_allow_overdraw(true);
        frame.paint(root, &mut display, &painter).unwrap();

        assert_eq!(
            display.get_pixel(EgPoint::new(0, 0)),
            Some(Rgb888::new(255, 0, 0,))
        );
        assert_eq!(
            display.get_pixel(EgPoint::new(5, 5)),
            Some(Rgb888::new(0, 0, 255,))
        );
    }

    #[test]
    fn rounded_background_does_not_fill_square_corner_pixel() {
        let painter = MonoTextPainter::new(&FONT_6X10, Color::WHITE);
        let mut frame = FrameArena::<8, 64>::default();

        let root = frame
            .mount(div().w(px(20)).h(px(20)).bg(Color::RED).rounded(px(6)))
            .unwrap();
        frame.layout(root, Size::new(px(20), px(20)), &painter);

        let mut display = MockDisplay::<Rgb888>::new();
        frame.paint(root, &mut display, &painter).unwrap();

        assert_eq!(display.get_pixel(EgPoint::new(0, 0)), None);
        assert_eq!(
            display.get_pixel(EgPoint::new(10, 10)),
            Some(Rgb888::new(255, 0, 0,))
        );
    }

    #[test]
    fn focused_border_color_is_paint_only_but_border_width_requires_layout() {
        struct App;

        impl App {
            fn clicked(&mut self, _: &ClickEvent, _cx: &mut Context<Self>) {}
        }

        impl Render for App {
            fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
                div()
                    .child(
                        div()
                            .id("paint")
                            .w(px(50))
                            .h(px(20))
                            .border(px(1))
                            .border_color(Color::BLUE)
                            .when_focused(|style| style.border_color(Color::RED))
                            .on_click(cx.listener(Self::clicked)),
                    )
                    .child(
                        div()
                            .id("layout")
                            .w(px(50))
                            .h(px(20))
                            .border(px(1))
                            .when_focused(|style| style.border(px(3)))
                            .on_click(cx.listener(Self::clicked)),
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
        assert!(runtime.focus_next());
        assert_eq!(runtime.take_invalidation(), Invalidation::Layout);
    }
}
