use core::marker::PhantomData;

use embedded_graphics::{
    Drawable,
    draw_target::DrawTargetExt,
    geometry::{Point as EgPoint, Size as EgSize},
    image::{Image as EgImage, ImageDrawable as EgImageDrawable},
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
    BoxPaint, Color, FontId, ImageId, ImageSource, LineHeight, Painter, Pixels, Point, Rect,
    ResolvedTextStyle, Size, TextAlign, TextMeasurer, px,
    text_layout::{ELLIPSIS, for_each_visible_text_line},
};

pub struct EmbeddedGraphicsImage<'image, D>
where
    D: EgDrawTarget,
{
    image: *const (),
    size: Size,
    draw_fn: unsafe fn(
        image: *const (),
        target: &mut D,
        origin: Point,
        clip: Option<Rect>,
    ) -> Result<(), D::Error>,
    _lifetime: PhantomData<&'image ()>,
}

impl<'image, D> Copy for EmbeddedGraphicsImage<'image, D> where D: EgDrawTarget {}
impl<'image, D> Clone for EmbeddedGraphicsImage<'image, D>
where
    D: EgDrawTarget,
{
    fn clone(&self) -> Self {
        *self
    }
}

impl<'image, D> EmbeddedGraphicsImage<'image, D>
where
    D: EgDrawTarget,
{
    pub fn new<T>(image: &'image T) -> Self
    where
        T: EgImageDrawable,
        T::Color: Into<D::Color>,
    {
        Self {
            image: core::ptr::from_ref(image).cast(),
            size: from_embedded_size(image.size()),
            draw_fn: draw_erased_image::<T, D>,
            _lifetime: PhantomData,
        }
    }

    pub const fn size(self) -> Size {
        self.size
    }

    pub const fn source(self, id: ImageId) -> ImageSource {
        ImageSource::new(id, self.size)
    }

    fn draw(self, target: &mut D, origin: Point, clip: Option<Rect>) -> Result<(), D::Error> {
        unsafe { (self.draw_fn)(self.image, target, origin, clip) }
    }
}

pub struct EmbeddedGraphicsPainter<
    'target,
    'font,
    'image,
    D,
    const FONTS: usize,
    const IMAGES: usize,
> where
    D: EgDrawTarget,
{
    target: &'target mut D,
    fonts: [&'font EgMonoFont<'font>; FONTS],
    images: [EmbeddedGraphicsImage<'image, D>; IMAGES],
}

impl<'target, 'font, 'image, D, const FONTS: usize, const IMAGES: usize>
    EmbeddedGraphicsPainter<'target, 'font, 'image, D, FONTS, IMAGES>
where
    D: EgDrawTarget,
{
    pub fn new(
        target: &'target mut D,
        fonts: [&'font EgMonoFont<'font>; FONTS],
        images: [EmbeddedGraphicsImage<'image, D>; IMAGES],
    ) -> Self {
        assert!(FONTS > 0, "at least one font must be registered");

        Self {
            target,
            fonts,
            images,
        }
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

    fn resolve_image(&self, id: ImageId) -> Option<EmbeddedGraphicsImage<'image, D>> {
        self.images.get(id.index()).copied()
    }
}

impl<D, const FONTS: usize, const IMAGES: usize> TextMeasurer
    for EmbeddedGraphicsPainter<'_, '_, '_, D, FONTS, IMAGES>
where
    D: EgDrawTarget,
{
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

impl<D, const FONTS: usize, const IMAGES: usize> Painter
    for EmbeddedGraphicsPainter<'_, '_, '_, D, FONTS, IMAGES>
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

    fn draw_image(
        &mut self,
        source: ImageSource,
        bounds: Rect,
        clip: Option<Rect>,
    ) -> Result<(), Self::Error> {
        if bounds.width().is_non_positive() || bounds.height().is_non_positive() {
            return Ok(());
        }

        let Some(image) = self.resolve_image(source.id()) else {
            debug_assert!(false, "image {:?} is not registered", source.id());
            return Ok(());
        };

        debug_assert_eq!(
            image.size(),
            source.size(),
            "registered image size differs from ImageSource size for {:?}",
            source.id()
        );

        let Some(image_clip) = (match clip {
            Some(clip) => clip.intersection(bounds),
            None => Some(bounds),
        }) else {
            return Ok(());
        };

        image.draw(self.target, bounds.origin, Some(image_clip))
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

fn to_embedded_point(point: Point) -> EgPoint {
    EgPoint::new(point.x.get(), point.y.get())
}

fn from_embedded_size(size: EgSize) -> Size {
    Size::new(
        px(i32::try_from(size.width).unwrap_or(i32::MAX)),
        px(i32::try_from(size.height).unwrap_or(i32::MAX)),
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

unsafe fn draw_erased_image<T, D>(
    image: *const (),
    target: &mut D,
    origin: Point,
    clip: Option<Rect>,
) -> Result<(), D::Error>
where
    D: EgDrawTarget,
    T: EgImageDrawable,
    T::Color: Into<D::Color>,
{
    let image = unsafe { &*image.cast::<T>() };
    let position = to_embedded_point(origin);
    let drawable = EgImage::new(image, position);
    let mut target = target.color_converted::<T::Color>();

    match clip {
        Some(clip) => {
            let clip = to_embedded_rect(clip);
            let mut clipped = target.clipped(&clip);
            drawable.draw(&mut clipped).map(|_| ())
        }
        None => drawable.draw(&mut target).map(|_| ()),
    }
}

#[cfg(test)]
mod tests {
    use embedded_graphics::{
        draw_target::DrawTarget as EgDrawTarget,
        geometry::{OriginDimensions, Point as EgPoint, Size as EgSize},
        image::ImageDrawable as EgImageDrawable,
        mock_display::MockDisplay,
        mono_font::ascii::FONT_6X10,
        pixelcolor::Rgb888,
        primitives::Rectangle as EgRectangle,
    };

    use crate::{
        backend::{
            EmbeddedGraphicsPainter,
            embedded_graphics::{EmbeddedGraphicsImage, aligned_line_x},
        },
        *,
    };

    #[test]
    fn embedded_graphics_backend_rasterizes_box() {
        let mut display = MockDisplay::<Rgb888>::new();

        {
            let mut painter = EmbeddedGraphicsPainter::new(&mut display, [&FONT_6X10], []);

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

        let painter = EmbeddedGraphicsPainter::new(&mut display, [&FONT_6X10], []);

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

        let painter = EmbeddedGraphicsPainter::new(&mut display, [&FONT_6X10], []);

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

        let painter = EmbeddedGraphicsPainter::new(&mut display, [&FONT_6X10], []);

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

        let painter = EmbeddedGraphicsPainter::new(&mut display, [&FONT_6X10], []);

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

        let painter = EmbeddedGraphicsPainter::new(&mut display, [&FONT_6X10], []);

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

    struct SolidTestImage {
        size: EgSize,
        color: Rgb888,
    }

    impl OriginDimensions for SolidTestImage {
        fn size(&self) -> EgSize {
            self.size
        }
    }

    impl EgImageDrawable for SolidTestImage {
        type Color = Rgb888;

        fn draw<D>(&self, target: &mut D) -> Result<(), D::Error>
        where
            D: EgDrawTarget<Color = Self::Color>,
        {
            target.fill_solid(&EgRectangle::new(EgPoint::zero(), self.size), self.color)
        }

        fn draw_sub_image<D>(&self, target: &mut D, area: &EgRectangle) -> Result<(), D::Error>
        where
            D: EgDrawTarget<Color = Self::Color>,
        {
            target.fill_solid(area, self.color)
        }
    }

    #[test]
    fn embedded_graphics_backend_draws_registered_image() {
        let bitmap = SolidTestImage {
            size: EgSize::new(4, 3),
            color: Rgb888::new(255, 0, 0),
        };
        let registered = EmbeddedGraphicsImage::new(&bitmap);
        let source = registered.source(ImageId::new(0));

        let mut display = MockDisplay::<Rgb888>::new();

        {
            let mut painter =
                EmbeddedGraphicsPainter::new(&mut display, [&FONT_6X10], [registered]);

            painter
                .draw_image(
                    source,
                    Rect::new(Point::new(px(2), px(3)), source.size()),
                    None,
                )
                .unwrap();
        }

        assert_eq!(
            display.get_pixel(EgPoint::new(2, 3,)),
            Some(Rgb888::new(255, 0, 0,))
        );
        assert_eq!(
            display.get_pixel(EgPoint::new(5, 5,)),
            Some(Rgb888::new(255, 0, 0,))
        );
        assert_eq!(display.get_pixel(EgPoint::new(1, 3,)), None);
    }

    #[test]
    fn embedded_graphics_backend_clips_image_to_layout_bounds() {
        let bitmap = SolidTestImage {
            size: EgSize::new(4, 4),
            color: Rgb888::new(0, 255, 0),
        };
        let registered = EmbeddedGraphicsImage::new(&bitmap);
        let source = registered.source(ImageId::new(0));

        let mut display = MockDisplay::<Rgb888>::new();

        {
            let mut painter =
                EmbeddedGraphicsPainter::new(&mut display, [&FONT_6X10], [registered]);

            painter
                .draw_image(
                    source,
                    Rect::new(Point::new(px(10), px(10)), Size::new(px(2), px(2))),
                    None,
                )
                .unwrap();
        }

        assert_eq!(
            display.get_pixel(EgPoint::new(10, 10,)),
            Some(Rgb888::new(0, 255, 0,))
        );
        assert_eq!(
            display.get_pixel(EgPoint::new(11, 11,)),
            Some(Rgb888::new(0, 255, 0,))
        );
        assert_eq!(display.get_pixel(EgPoint::new(12, 10,)), None);
        assert_eq!(display.get_pixel(EgPoint::new(10, 12,)), None);
    }

    #[test]
    fn embedded_graphics_backend_combines_image_bounds_with_ancestor_clip() {
        let bitmap = SolidTestImage {
            size: EgSize::new(6, 4),

            color: Rgb888::new(0, 0, 255),
        };
        let registered = EmbeddedGraphicsImage::new(&bitmap);
        let source = registered.source(ImageId::new(0));

        let mut display = MockDisplay::<Rgb888>::new();

        {
            let mut painter =
                EmbeddedGraphicsPainter::new(&mut display, [&FONT_6X10], [registered]);

            painter
                .draw_image(
                    source,
                    Rect::new(Point::new(px(4), px(4)), source.size()),
                    Some(Rect::new(Point::new(px(5), px(5)), Size::new(px(2), px(2)))),
                )
                .unwrap();
        }

        assert_eq!(
            display.get_pixel(EgPoint::new(5, 5,)),
            Some(Rgb888::new(0, 0, 255,))
        );
        assert_eq!(
            display.get_pixel(EgPoint::new(6, 6,)),
            Some(Rgb888::new(0, 0, 255,))
        );
        assert_eq!(display.get_pixel(EgPoint::new(4, 4,)), None);
        assert_eq!(display.get_pixel(EgPoint::new(7, 5,)), None);
    }
}
