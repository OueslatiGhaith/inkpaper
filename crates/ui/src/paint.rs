use embedded_graphics::{
    Drawable,
    draw_target::{DrawTarget, DrawTargetExt},
    geometry::{Point as EgPoint, Size as EgSize},
    mono_font::{MonoFont, MonoTextStyle},
    pixelcolor::Rgb888,
    primitives::{Primitive, PrimitiveStyle, Rectangle},
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

            match self.node(node).parent {
                Some(parent) => node = parent,
                None => return None,
            }
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
            crate::NodeKind::Div { style } => {
                let Some(background) = style.background else {
                    return Ok(());
                };

                let bounds = node.layout.bounds;
                if bounds.width().0 <= 0 || bounds.height().0 <= 0 {
                    return Ok(());
                }

                let rectangle = to_embedded_rect(bounds);
                rectangle
                    .into_styled(PrimitiveStyle::with_fill(to_rgb888(background)))
                    .draw(target)?;

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
            current = self.next_paint_node(node);
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
}
