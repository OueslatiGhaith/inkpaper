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

use crate::{
    BoxPaint, Color, FontId, LineHeight, Painter, Pixels, Rect, ResolvedTextStyle, Size, TextAlign,
    TextMeasurer, px,
    text_layout::{ELLIPSIS, for_each_text_line, for_each_visible_text_line},
};

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
    fn measure_text(&self, text: &str, style: ResolvedTextStyle, max_size: Size) -> Size {
        if text.is_empty() {
            return Size::ZERO;
        }

        let font = self.resolve_font(style.font);
        let glyph_height = font_character_height(font);
        let line_advance = text_line_advance(font, style);

        let mut longest_line = Pixels::ZERO;
        let mut line_count: i32 = 0;

        for_each_visible_text_line(
            text,
            style.wrap,
            max_size.width,
            style.max_lines,
            style.overflow,
            |line| measure_mono_line(font, line),
            |line| measure_mono_line_with_ellipsis(font, line),
            |line| {
                longest_line = longest_line.max(line.width);
                line_count = line_count.saturating_add(1);
            },
        );

        if line_count == 0 {
            return Size::ZERO;
        }

        let height = glyph_height.saturating_add(line_advance.saturating_mul(line_count - 1));

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
        style: ResolvedTextStyle,
        clip: Option<Rect>,
    ) -> Result<(), Self::Error> {
        if bounds.width().is_non_positive() || bounds.height().is_non_positive() {
            return Ok(());
        }

        let Some(text_clip) = (match clip {
            Some(clip) => clip.intersection(bounds),

            None => Some(bounds),
        }) else {
            return Ok(());
        };

        let font = self.resolve_font(style.font);
        let mut target = self.target.color_converted();
        let clip = to_embedded_rect(text_clip);
        let mut clipped = target.clipped(&clip);

        draw_text_to(&mut clipped, text, bounds, font, style)
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
    style: ResolvedTextStyle,
) -> Result<(), D::Error>
where
    D: EgDrawTarget<Color = EgRgb888>,
{
    if text.is_empty() {
        return Ok(());
    }

    let line_advance = text_line_advance(font, style);
    let color = to_rgb888(style.color);

    let mut y = bounds.origin.y;
    let mut error = None;

    for_each_visible_text_line(
        text,
        style.wrap,
        bounds.width(),
        style.max_lines,
        style.overflow,
        |line| measure_mono_line(font, line),
        |line| measure_mono_line_with_ellipsis(font, line),
        |line| {
            if error.is_some() {
                return;
            }

            let x = aligned_line_x(bounds, line.width, style.align);

            if !line.text.is_empty() {
                let text_style = EgMonoTextStyle::new(font, color);

                if let Err(draw_error) = EgText::with_baseline(
                    line.text,
                    EgPoint::new(x.get(), y.get()),
                    text_style,
                    EgBaseline::Top,
                )
                .draw(target)
                .map(|_| ())
                {
                    error = Some(draw_error);
                    return;
                }
            }

            if line.ellipsis {
                let ellipsis_x = x + mono_text_advance(font, line.text);
                let text_style = EgMonoTextStyle::new(font, color);

                if let Err(draw_error) = EgText::with_baseline(
                    ELLIPSIS,
                    EgPoint::new(ellipsis_x.get(), y.get()),
                    text_style,
                    EgBaseline::Top,
                )
                .draw(target)
                .map(|_| ())
                {
                    error = Some(draw_error);
                    return;
                }
            }

            y += line_advance;
        },
    );

    match error {
        Some(error) => Err(error),
        None => Ok(()),
    }
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

fn text_line_advance(font: &EgMonoFont<'_>, style: ResolvedTextStyle) -> Pixels {
    match style.line_height {
        LineHeight::Normal => font_character_height(font),
        LineHeight::Pixels(height) => height.non_negative(),
    }
}

fn mono_character_count(text: &str) -> i32 {
    i32::try_from(text.chars().count()).unwrap_or(i32::MAX)
}

fn measure_mono_characters(font: &EgMonoFont<'_>, characters: i32) -> Pixels {
    if characters <= 0 {
        return px(0);
    }

    let character_width = font_character_width(font);
    let spacing = font_character_spacing(font);

    character_width
        .saturating_add(spacing)
        .saturating_mul(characters)
        .saturating_sub(spacing)
}

fn measure_mono_line(font: &EgMonoFont<'_>, text: &str) -> Pixels {
    measure_mono_characters(font, mono_character_count(text))
}

fn measure_mono_line_with_ellipsis(font: &EgMonoFont<'_>, text: &str) -> Pixels {
    let characters = mono_character_count(text).saturating_add(mono_character_count(ELLIPSIS));

    measure_mono_characters(font, characters)
}

fn mono_text_advance(font: &EgMonoFont<'_>, text: &str) -> Pixels {
    let characters = mono_character_count(text);
    if characters <= 0 {
        return px(0);
    }

    font_character_width(font)
        .saturating_add(font_character_spacing(font))
        .saturating_mul(characters)
}

fn aligned_line_x(bounds: Rect, line_width: Pixels, align: TextAlign) -> Pixels {
    let remaining = (bounds.width() - line_width).non_negative();
    match align {
        TextAlign::Start => bounds.origin.x,
        TextAlign::Center => bounds.origin.x + remaining / 2,
        TextAlign::End => bounds.origin.x + remaining,
    }
}

#[cfg(test)]
mod tests {
    use embedded_graphics::{
        geometry::Point as EgPoint, mock_display::MockDisplay, mono_font::ascii::FONT_6X10,
        pixelcolor::Rgb888,
    };

    use crate::{
        backend::{EmbeddedGraphicsPainter, embedded_graphics::aligned_line_x},
        *,
    };

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

    #[test]
    fn text_measurement_wraps_at_word_boundaries() {
        let mut display = MockDisplay::<Rgb888>::new();

        let painter = EmbeddedGraphicsPainter::new(&mut display, [&FONT_6X10]);

        let size = painter.measure_text(
            "hello world",
            ResolvedTextStyle {
                wrap: TextWrap::Word,
                ..ResolvedTextStyle::default()
            },
            Size::new(px(30), px(100)),
        );

        assert_eq!(size, Size::new(px(30), px(20),));
    }

    #[test]
    fn custom_line_height_affects_multiline_measurement() {
        let mut display = MockDisplay::<Rgb888>::new();

        let painter = EmbeddedGraphicsPainter::new(&mut display, [&FONT_6X10]);

        let size = painter.measure_text(
            "first\nsecond",
            ResolvedTextStyle {
                line_height: LineHeight::Pixels(px(15)),
                ..ResolvedTextStyle::default()
            },
            Size::new(px(100), px(100)),
        );

        assert_eq!(size.height, px(25));
    }

    #[test]
    fn centered_text_line_is_offset_inside_bounds() {
        let bounds = Rect::new(Point::new(px(10), px(5)), Size::new(px(100), px(20)));

        assert_eq!(aligned_line_x(bounds, px(40), TextAlign::Center,), px(40));
        assert_eq!(aligned_line_x(bounds, px(40), TextAlign::End,), px(70));
        assert_eq!(aligned_line_x(bounds, px(40), TextAlign::Start,), px(10));
    }

    #[test]
    fn max_lines_limits_measured_height() {
        let mut display = MockDisplay::<Rgb888>::new();

        let painter = EmbeddedGraphicsPainter::new(&mut display, [&FONT_6X10]);

        let size = painter.measure_text(
            "hello world again",
            ResolvedTextStyle {
                wrap: TextWrap::Word,
                max_lines: TextMaxLines::Limited(2),
                ..ResolvedTextStyle::default()
            },
            Size::new(px(30), px(100)),
        );

        assert_eq!(size.height, px(20));
    }

    #[test]
    fn ellipsis_respects_available_width() {
        let mut display = MockDisplay::<Rgb888>::new();

        let painter = EmbeddedGraphicsPainter::new(&mut display, [&FONT_6X10]);

        let size = painter.measure_text(
            "abcdefghij",
            ResolvedTextStyle {
                overflow: TextOverflow::Ellipsis,
                ..ResolvedTextStyle::default()
            },
            Size::new(px(30), px(100)),
        );

        assert_eq!(size, Size::new(px(30), px(10),));
    }

    #[test]
    fn wrapped_text_can_be_clamped_with_ellipsis() {
        let mut display = MockDisplay::<Rgb888>::new();

        let painter = EmbeddedGraphicsPainter::new(&mut display, [&FONT_6X10]);

        let size = painter.measure_text(
            "hello world again",
            ResolvedTextStyle {
                wrap: TextWrap::Word,
                max_lines: TextMaxLines::Limited(1),
                overflow: TextOverflow::Ellipsis,
                ..ResolvedTextStyle::default()
            },
            Size::new(px(30), px(100)),
        );

        assert_eq!(size, Size::new(px(30), px(10),));
    }
}
