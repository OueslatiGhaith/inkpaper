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

use crate::{BoxPaint, Color, FontId, Painter, Pixels, Rect, Size, TextMeasurer, TextStyle, px};

pub struct EmbeddedGraphicsPainter<'target, 'font, D, const FONTS: usize> {
    target: &'target mut D,
    fonts: [&'font EgMonoFont<'font>; FONTS],
}

impl<'target, 'font, D, const FONTS: usize> EmbeddedGraphicsPainter<'target, 'font, D, FONTS> {
    pub fn new(target: &'target mut D, fonts: [&'font EgMonoFont<'font>; FONTS]) -> Self {
        assert!(FONTS > 0, "at least one font must be registered");

        Self { target, fonts }
    }

    pub fn target_mut(&mut self) -> &mut D {
        self.target
    }

    fn resolve_font(&self, font: FontId) -> &'font EgMonoFont<'font> {
        self.fonts
            .get(font.index())
            .copied()
            .unwrap_or(self.fonts[0])
    }
}

impl<D, const FONTS: usize> TextMeasurer for EmbeddedGraphicsPainter<'_, '_, D, FONTS> {
    fn measure(&self, text: &str, style: TextStyle, max_size: Size) -> Size {
        if text.is_empty() {
            return Size::ZERO;
        }

        let font = self.resolve_font(style.font);
        let character_width = font_character_width(font);
        let character_spacing = font_character_spacing(font);
        let glyph_height = font_character_height(font);
        let line_advance = text_line_advance(font, style);

        let mut longest_line = px(0);
        let mut line_count = 0i32;

        for line in text.split('\n') {
            let characters = i32::try_from(line.chars().count()).unwrap_or(i32::MAX);
            let width = if characters == 0 {
                px(0)
            } else {
                character_width
                    .saturating_add(character_spacing)
                    .saturating_mul(characters)
                    .saturating_sub(character_spacing)
            };

            longest_line = longest_line.max(width);
            line_count = line_count.saturating_add(1);
        }

        let height = if line_count <= 0 {
            px(0)
        } else {
            glyph_height
                .saturating_add(line_advance)
                .saturating_mul(line_count - 1)
        };

        Size::new(
            longest_line
                .non_negative()
                .min(max_size.width.non_negative()),
            height.non_negative().min(max_size.height.non_negative()),
        )
    }
}

impl<D, const FONTS: usize> Painter for EmbeddedGraphicsPainter<'_, '_, D, FONTS>
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
        bounds: Rect,
        style: TextStyle,
        clip: Option<Rect>,
    ) -> Result<(), Self::Error> {
        let font = self.resolve_font(style.font);
        let mut target = self.target.color_converted();

        if let Some(clip) = clip {
            let clip = to_embedded_rect(clip);
            let mut clipped = target.clipped(&clip);

            draw_text_to(&mut clipped, text, bounds, font, style)
        } else {
            draw_text_to(&mut target, text, bounds, font, style)
        }
    }
}

fn to_rgb888(color: Color) -> EgRgb888 {
    EgRgb888::new(color.r, color.g, color.b)
}

fn to_embedded_rect(rect: Rect) -> EgRectangle {
    let width = u32::try_from(rect.width().non_negative().get()).unwrap_or(u32::MAX);
    let height = u32::try_from(rect.height().non_negative().get()).unwrap_or(u32::MAX);

    EgRectangle::new(
        EgPoint::new(rect.x().get(), rect.y().get()),
        EgSize::new(width, height),
    )
}

fn draw_box_to<D>(target: &mut D, bounds: Rect, paint: BoxPaint) -> Result<(), D::Error>
where
    D: EgDrawTarget<Color = EgRgb888>,
{
    if bounds.width().is_non_positive() || bounds.height().is_non_positive() {
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
        let width = u32::try_from(border.width.non_negative().get()).unwrap_or(u32::MAX);
        if width > 0 {
            style = style
                .stroke_color(to_rgb888(border.color))
                .stroke_width(width);
        }
    }

    let style = style.build();
    let rectangle = to_embedded_rect(bounds);
    let radius = u32::try_from(paint.radius.non_negative().get()).unwrap_or(u32::MAX);

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
    bounds: Rect,
    font: &EgMonoFont<'_>,
    style: TextStyle,
) -> Result<(), D::Error>
where
    D: EgDrawTarget<Color = EgRgb888>,
{
    if text.is_empty() {
        return Ok(());
    }

    let text_style = EgMonoTextStyle::new(font, to_rgb888(style.color));
    let line_advance = text_line_advance(font, style);

    let mut origin = bounds.origin;
    for line in text.split('\n') {
        if !line.is_empty() {
            EgText::with_baseline(
                line,
                EgPoint::new(origin.x.get(), origin.y.get()),
                text_style,
                EgBaseline::Top,
            )
            .draw(target)?;
        }

        origin.y += line_advance;
    }

    Ok(())
}

fn font_character_width(font: &EgMonoFont<'_>) -> Pixels {
    px(i32::try_from(font.character_size.width).unwrap_or(i32::MAX))
}

fn font_character_height(font: &EgMonoFont<'_>) -> Pixels {
    px(i32::try_from(font.character_size.height).unwrap_or(i32::MAX))
}

fn font_character_spacing(font: &EgMonoFont<'_>) -> Pixels {
    px(i32::try_from(font.character_spacing).unwrap_or(i32::MAX))
}

fn text_line_advance(font: &EgMonoFont<'_>, style: TextStyle) -> Pixels {
    style
        .line_height
        .unwrap_or_else(|| font_character_height(font))
        .non_negative()
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
            let mut painter = EmbeddedGraphicsPainter::new(&mut display, [&FONT_6X10]);

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
