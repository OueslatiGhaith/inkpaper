use embedded_graphics::{
    Drawable,
    draw_target::DrawTargetExt,
    geometry::{Point as EgPoint, Size as EgSize},
    mono_font::{MonoFont as EgMonoFont, MonoTextStyle as EgMonoTextStyle},
    pixelcolor::Rgb888 as EgRgb888,
    prelude::DrawTarget as EgDrawTarget,
    primitives::{
        Primitive, PrimitiveStyleBuilder as EgPrimitiveStyleBuilder, Rectangle as EgRectangle,
        RoundedRectangle as EgRoundedRectangle, StrokeAlignment as EgStrokeAlignment,
    },
    text::{Baseline as EgBaseline, Text as EgText},
};

use crate::{BoxPaint, Color, Painter, Point, Rect, Size, TextMeasurer, px};

pub struct EmbeddedGraphicsPainter<'target, 'font, D> {
    target: &'target mut D,
    font: &'font EgMonoFont<'font>,
    // TODO: temporary until we implement text styles
    text_color: Color,
}

impl<'target, 'font, D> EmbeddedGraphicsPainter<'target, 'font, D> {
    pub fn new(target: &'target mut D, font: &'font EgMonoFont<'font>, text_color: Color) -> Self {
        Self {
            target,
            font,
            text_color,
        }
    }

    pub fn target_mut(&mut self) -> &mut D {
        self.target
    }

    pub fn set_text_color(&mut self, color: Color) {
        self.text_color = color;
    }
}

impl<D> TextMeasurer for EmbeddedGraphicsPainter<'_, '_, D> {
    fn measure(&self, text: &str, max_size: Size) -> Size {
        if text.is_empty() {
            return Size::ZERO;
        }

        let mut longest_line = 0;
        let mut lines = 0;

        for line in text.split('\n') {
            longest_line = longest_line.max(line.chars().count());
            lines += 1;
        }

        let character_width = self.font.character_size.width;
        let character_height = self.font.character_size.height;
        let spacing = self.font.character_spacing;
        let characters = u32::try_from(longest_line).unwrap_or(u32::MAX);
        let lines = u32::try_from(lines).unwrap_or(u32::MAX);
        let height = lines.saturating_mul(character_height);
        let width = if characters == 0 {
            0
        } else {
            characters
                .saturating_mul(character_width.saturating_add(spacing))
                .saturating_sub(spacing)
        };

        let width = i32::try_from(width)
            .unwrap_or(i32::MAX)
            .min(max_size.width.0.max(0));
        let height = i32::try_from(height)
            .unwrap_or(i32::MAX)
            .min(max_size.height.0.max(0));

        Size::new(px(width), px(height))
    }
}

impl<D> Painter for EmbeddedGraphicsPainter<'_, '_, D>
where
    D: EgDrawTarget,
    D::Color: From<EgRgb888>,
{
    type Error = D::Error;

    fn draw_box(
        &mut self,
        bounds: Rect,
        paint: BoxPaint,
        clip: Option<Rect>,
    ) -> Result<(), Self::Error> {
        let mut target = self.target.color_converted::<EgRgb888>();

        if let Some(clip) = clip {
            let clip = to_embedded_rect(clip);
            let mut clipped = target.clipped(&clip);
            draw_box_to(&mut clipped, bounds, paint)
        } else {
            draw_box_to(&mut target, bounds, paint)
        }
    }

    fn draw_text(
        &mut self,
        text: &str,
        origin: Point,
        clip: Option<Rect>,
    ) -> Result<(), Self::Error> {
        let mut target = self.target.color_converted::<EgRgb888>();

        if let Some(clip) = clip {
            let clip = to_embedded_rect(clip);
            let mut clipped = target.clipped(&clip);
            draw_text_to(&mut clipped, text, origin, self.font, self.text_color)
        } else {
            draw_text_to(&mut target, text, origin, self.font, self.text_color)
        }
    }
}

fn to_rgb888(color: Color) -> EgRgb888 {
    EgRgb888::new(color.r, color.g, color.b)
}

fn to_embedded_rect(rect: Rect) -> EgRectangle {
    let width = u32::try_from(rect.width().0.max(0)).unwrap_or(u32::MAX);
    let height = u32::try_from(rect.height().0.max(0)).unwrap_or(u32::MAX);

    EgRectangle::new(
        EgPoint::new(rect.x().0, rect.y().0),
        EgSize::new(width, height),
    )
}

fn draw_box_to<D>(target: &mut D, bounds: Rect, paint: BoxPaint) -> Result<(), D::Error>
where
    D: EgDrawTarget<Color = EgRgb888>,
{
    if bounds.width().0 <= 0 || bounds.height().0 <= 0 {
        return Ok(());
    }
    if paint.background.is_none() && paint.border.is_none() {
        return Ok(());
    }

    let mut style = EgPrimitiveStyleBuilder::new().stroke_alignment(EgStrokeAlignment::Inside);
    if let Some(background) = paint.background {
        style = style.fill_color(to_rgb888(background));
    }
    if let Some(border) = paint.border {
        let width = u32::try_from(border.width.0.max(0)).unwrap_or(u32::MAX);
        if width > 0 {
            style = style
                .stroke_color(to_rgb888(border.color))
                .stroke_width(width);
        }
    }

    let style = style.build();
    let rectangle = to_embedded_rect(bounds);
    let radius = u32::try_from(paint.radius.0.max(0)).unwrap_or(u32::MAX);

    if radius == 0 {
        rectangle.into_styled(style).draw(target)?;
    } else {
        EgRoundedRectangle::with_equal_corners(rectangle, EgSize::new(radius, radius))
            .into_styled(style)
            .draw(target)?;
    }

    Ok(())
}

fn draw_text_to<D>(
    target: &mut D,
    text: &str,
    origin: Point,
    font: &EgMonoFont<'_>,
    color: Color,
) -> Result<(), D::Error>
where
    D: EgDrawTarget<Color = EgRgb888>,
{
    let style = EgMonoTextStyle::new(font, to_rgb888(color));
    EgText::with_baseline(
        text,
        EgPoint::new(origin.x.0, origin.y.0),
        style,
        EgBaseline::Top,
    )
    .draw(target)
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    use embedded_graphics::{
        geometry::Point as EgPoint, mock_display::MockDisplay, mono_font::ascii::FONT_6X10,
        pixelcolor::Rgb888,
    };

    use crate::{backend::EmbeddedGraphicsPainter, *};

    #[test]
    fn embedded_graphics_backend_rasterizes_box() {
        let mut display = MockDisplay::<Rgb888>::new();

        {
            let mut painter = EmbeddedGraphicsPainter::new(&mut display, &FONT_6X10, Color::WHITE);

            painter
                .draw_box(
                    Rect::new(Point::new(px(0), px(0)), Size::new(px(10), px(10))),
                    BoxPaint {
                        background: Some(Color::BLUE),
                        border: Some(BorderPaint {
                            width: px(1),
                            color: Color::RED,
                        }),
                        radius: px(0),
                    },
                    None,
                )
                .unwrap();
        }

        assert_eq!(
            display.get_pixel(EgPoint::new(0, 0,)),
            Some(Rgb888::new(255, 0, 0,))
        );
        assert_eq!(
            display.get_pixel(EgPoint::new(5, 5,)),
            Some(Rgb888::new(0, 0, 255,))
        );
    }
}
