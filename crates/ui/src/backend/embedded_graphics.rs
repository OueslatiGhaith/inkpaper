use core::marker::PhantomData;

use embedded_graphics::{
    Drawable, Pixel as EgPixel,
    draw_target::DrawTargetExt,
    geometry::{Point as EgPoint, Size as EgSize},
    image::{GetPixel as EgGetPixel, Image as EgImage, ImageDrawable as EgImageDrawable},
    pixelcolor::{Rgb888 as EgRgb888, RgbColor},
    prelude::DrawTarget as EgDrawTarget,
    primitives::{
        Circle as EgCircle, Line as EgLine, Primitive, PrimitiveStyle as EgPrimitiveStyle,
        PrimitiveStyleBuilder as EgPrimitiveStyleBuilder, Rectangle as EgRectangle,
        RoundedRectangle as EgRoundedRectangle, StrokeAlignment as EgStrokeAlignment,
    },
};

use crate::{
    BoxPaint, CanvasPainter, Color, DamageRegion, FontFace, FontId, FontRegistry, FontResources,
    GlyphBitmap, GlyphCacheError, ImageFit, ImageId, ImageSource, LineHeight, Painter, Pixels,
    Point, Rect, ResolvedTextStyle, ShapeError, ShapeState, ShapedGlyph, ShapedRun, SimpleShaper,
    Size, TextAlign, TextDirection, TextMeasurer, fitted_image_bounds, px,
    text_layout::{ELLIPSIS, for_each_visible_text_line},
};

const SHAPED_LINE_GLYPH_CAPACITY: usize = 128;

#[derive(Debug)]
pub enum EmbeddedGraphicsError<E> {
    Target(E),
    Font(GlyphCacheError),
    Shape(ShapeError),
}

#[derive(Debug)]
pub enum CoverageMode<D> {
    BinaryThreshold,
    OrderedDither4x4,
    AlphaBlend {
        read_pixel: fn(&D, EgPoint) -> Option<EgRgb888>,
    },
}

impl<D> Copy for CoverageMode<D> {}
impl<D> Clone for CoverageMode<D> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<D> CoverageMode<D> {
    pub const fn binary_threshold() -> Self {
        Self::BinaryThreshold
    }

    pub const fn ordered_dither_4x4() -> Self {
        Self::OrderedDither4x4
    }

    pub const fn alpha_blend(read_pixel: fn(&D, EgPoint) -> Option<EgRgb888>) -> Self {
        Self::AlphaBlend { read_pixel }
    }
}

pub struct EmbeddedGraphicsImage<'image, D>
where
    D: EgDrawTarget,
{
    image: *const (),
    size: Size,
    #[allow(clippy::type_complexity)]
    draw_fn: unsafe fn(
        image: *const (),
        target: &mut D,
        origin: Point,
        clip: Option<Rect>,
    ) -> Result<(), D::Error>,
    #[allow(clippy::type_complexity)]
    draw_scaled_fn: unsafe fn(
        image: *const (),
        target: &mut D,
        destination: Rect,
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
        T: EgImageDrawable + EgGetPixel<Color = <T as EgImageDrawable>::Color>,
        <T as EgImageDrawable>::Color: Into<D::Color>,
    {
        Self {
            image: core::ptr::from_ref(image).cast(),
            size: from_embedded_size(image.size()),
            draw_fn: draw_erased_image::<T, D>,
            draw_scaled_fn: draw_erased_scaled_image::<T, D>,
            _lifetime: PhantomData,
        }
    }

    pub const fn size(self) -> Size {
        self.size
    }

    pub const fn source(self, id: ImageId) -> ImageSource {
        ImageSource::new(id, self.size)
    }

    fn draw(
        self,
        target: &mut D,
        bounds: Rect,
        fit: ImageFit,
        clip: Option<Rect>,
    ) -> Result<(), D::Error> {
        let destination = fitted_image_bounds(self.size, bounds, fit);
        if destination.width().is_non_positive() || destination.height().is_non_positive() {
            return Ok(());
        }
        if fit == ImageFit::None || destination.size == self.size {
            return unsafe { (self.draw_fn)(self.image, target, destination.origin, clip) };
        }

        unsafe { (self.draw_scaled_fn)(self.image, target, destination, clip) }
    }
}

struct EmbeddedGraphicsCanvasPainter<'target, D>
where
    D: EgDrawTarget<Color = EgRgb888>,
{
    target: &'target mut D,
    origin: Point,
    error: Option<D::Error>,
}

impl<'target, D> EmbeddedGraphicsCanvasPainter<'target, D>
where
    D: EgDrawTarget<Color = EgRgb888>,
{
    fn new(target: &'target mut D, origin: Point) -> Self {
        Self {
            target,
            origin,
            error: None,
        }
    }

    fn translated_point(&self, point: Point) -> Point {
        Point::new(self.origin.x + point.x, self.origin.y + point.y)
    }

    fn translated_rect(&self, rect: Rect) -> Rect {
        Rect::new(self.translated_point(rect.origin), rect.size)
    }

    fn record(&mut self, result: Result<(), D::Error>) {
        if self.error.is_none()
            && let Err(error) = result
        {
            self.error = Some(error);
        }
    }

    fn finish(self) -> Result<(), D::Error> {
        match self.error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

impl<D> CanvasPainter for EmbeddedGraphicsCanvasPainter<'_, D>
where
    D: EgDrawTarget<Color = EgRgb888>,
{
    fn fill_rect(&mut self, rect: Rect, color: Color) {
        if self.error.is_some() || rect.width().is_non_positive() || rect.height().is_non_positive()
        {
            return;
        }

        let rect = self.translated_rect(rect);

        let style = EgPrimitiveStyle::with_fill(to_rgb888(color));

        let result = to_embedded_rect(rect).into_styled(style).draw(self.target);

        self.record(result);
    }

    fn stroke_rect(&mut self, rect: Rect, width: Pixels, color: Color) {
        if self.error.is_some()
            || rect.width().is_non_positive()
            || rect.height().is_non_positive()
            || width.is_non_positive()
        {
            return;
        }

        let rect = self.translated_rect(rect);
        let width = u32::try_from(width.non_negative().get()).unwrap_or(u32::MAX);

        let style = EgPrimitiveStyleBuilder::new()
            .stroke_color(to_rgb888(color))
            .stroke_width(width)
            .stroke_alignment(EgStrokeAlignment::Inside)
            .build();

        let result = to_embedded_rect(rect).into_styled(style).draw(self.target);

        self.record(result);
    }

    fn line(&mut self, start: Point, end: Point, width: Pixels, color: Color) {
        if self.error.is_some() || width.is_non_positive() {
            return;
        }

        let start = self.translated_point(start);
        let end = self.translated_point(end);
        let width = u32::try_from(width.non_negative().get()).unwrap_or(u32::MAX);

        let style = EgPrimitiveStyle::with_stroke(to_rgb888(color), width);

        let result = EgLine::new(to_embedded_point(start), to_embedded_point(end))
            .into_styled(style)
            .draw(self.target);

        self.record(result);
    }

    fn fill_circle(&mut self, center: Point, radius: Pixels, color: Color) {
        if self.error.is_some() || radius.is_non_positive() {
            return;
        }

        let center = self.translated_point(center);
        let diameter = radius.non_negative().get().saturating_mul(2);
        let diameter = u32::try_from(diameter).unwrap_or(u32::MAX);

        let style = EgPrimitiveStyle::with_fill(to_rgb888(color));

        let result = EgCircle::with_center(to_embedded_point(center), diameter)
            .into_styled(style)
            .draw(self.target);

        self.record(result);
    }

    fn stroke_circle(&mut self, center: Point, radius: Pixels, width: Pixels, color: Color) {
        if self.error.is_some() || radius.is_non_positive() || width.is_non_positive() {
            return;
        }

        let center = self.translated_point(center);
        let diameter = radius.non_negative().get().saturating_mul(2);
        let diameter = u32::try_from(diameter).unwrap_or(u32::MAX);
        let width = u32::try_from(width.non_negative().get()).unwrap_or(u32::MAX);

        let style = EgPrimitiveStyleBuilder::new()
            .stroke_color(to_rgb888(color))
            .stroke_width(width)
            .stroke_alignment(EgStrokeAlignment::Inside)
            .build();

        let result = EgCircle::with_center(to_embedded_point(center), diameter)
            .into_styled(style)
            .draw(self.target);

        self.record(result);
    }
}

pub struct EmbeddedGraphicsPainter<
    'target,
    'resources,
    'font,
    'storage,
    'image,
    D,
    const FONTS: usize,
    const GLYPH_SLOTS: usize,
    const IMAGES: usize,
> where
    D: EgDrawTarget,
{
    target: &'target mut D,
    font_resources: &'resources mut FontResources<'font, 'storage, FONTS, GLYPH_SLOTS>,
    images: [EmbeddedGraphicsImage<'image, D>; IMAGES],
    coverage_mode: CoverageMode<D>,
}

impl<
    'target,
    'resources,
    'font,
    'storage,
    'image,
    D,
    const FONTS: usize,
    const GLYPH_SLOTS: usize,
    const IMAGES: usize,
>
    EmbeddedGraphicsPainter<
        'target,
        'resources,
        'font,
        'storage,
        'image,
        D,
        FONTS,
        GLYPH_SLOTS,
        IMAGES,
    >
where
    D: EgDrawTarget,
{
    pub fn new(
        target: &'target mut D,
        font_resources: &'resources mut FontResources<'font, 'storage, FONTS, GLYPH_SLOTS>,
        images: [EmbeddedGraphicsImage<'image, D>; IMAGES],
    ) -> Self {
        assert!(
            !font_resources.is_empty(),
            "at least one font must be registered",
        );

        Self {
            target,
            font_resources,
            images,
            coverage_mode: CoverageMode::BinaryThreshold,
        }
    }

    pub fn with_coverage_mode(mut self, coverage_mode: CoverageMode<D>) -> Self {
        self.coverage_mode = coverage_mode;
        self
    }

    pub fn target_mut(&mut self) -> &mut D {
        self.target
    }

    fn resolve_font(&self, font: FontId) -> (FontId, &'font dyn FontFace) {
        self.font_resources
            .resolve(font)
            .expect("EmbeddedGraphicsPainter requires a default font")
    }

    fn resolve_image(&self, id: ImageId) -> Option<EmbeddedGraphicsImage<'image, D>> {
        self.images.get(id.index()).copied()
    }

    pub fn clear_damage(&mut self, damage: DamageRegion, color: Color) -> Result<(), D::Error>
    where
        D::Color: From<EgRgb888>,
    {
        if damage.is_none() {
            return Ok(());
        }

        let color = to_rgb888(color);

        // a full invalidation should use the `DrawTarget`'s native clear operation.
        // besides being simpler, individual displays may have a considerabely more
        // efficient implementation for this case
        if damage.is_full() {
            let mut target = self.target.color_converted::<EgRgb888>();
            return target.clear(color);
        }

        // runtime damage can considerabely extend beyond the physical target.
        // clip it here so a backend never receives out-of-bounds partial clear rects
        let target_bounds = self.target.bounding_box();
        let target_bounds = Rect::new(
            Point::new(px(target_bounds.top_left.x), px(target_bounds.top_left.y)),
            from_embedded_size(target_bounds.size),
        );
        let damage = damage.clipped_to(target_bounds);
        if damage.is_none() {
            return Ok(());
        }

        let mut target = self.target.color_converted::<EgRgb888>();

        for &rect in damage.rects() {
            target.fill_solid(&to_embedded_rect(rect), color)?;
        }

        Ok(())
    }
}

impl<D, const FONTS: usize, const GLYPH_SLOTS: usize, const IMAGES: usize> TextMeasurer
    for EmbeddedGraphicsPainter<'_, '_, '_, '_, '_, D, FONTS, GLYPH_SLOTS, IMAGES>
where
    D: EgDrawTarget,
{
    fn measure_text(&self, text: &str, style: ResolvedTextStyle, max_size: Size) -> Size {
        if text.is_empty() {
            return Size::ZERO;
        }

        let (font_id, font) = self.resolve_font(style.font);
        let registry = self.font_resources.registry();
        let size_px = font_size_px(style);
        let glyph_height = font.metrics(size_px).line_height();
        let line_advance = text_line_advance(font, size_px, style);
        let mut longest_line = Pixels::ZERO;
        let mut line_count = 0i32;

        for_each_visible_text_line(
            text,
            style.wrap,
            max_size.width,
            style.max_lines,
            style.overflow,
            |line| measure_shaped_line(&registry, font_id, size_px, line),
            |line| measure_shaped_line_with_ellipsis(&registry, font_id, size_px, line),
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

impl<D, const FONTS: usize, const GLYPH_SLOTS: usize, const IMAGES: usize> Painter
    for EmbeddedGraphicsPainter<'_, '_, '_, '_, '_, D, FONTS, GLYPH_SLOTS, IMAGES>
where
    D: EgDrawTarget,
    D::Color: From<EgRgb888>,
{
    type Error = EmbeddedGraphicsError<D::Error>;

    fn draw_box(
        &mut self,
        bounds: Rect,
        paint: BoxPaint,
        clip: Option<Rect>,
    ) -> Result<(), Self::Error> {
        let mut target = self.target.color_converted::<EgRgb888>();

        let result = if let Some(clip) = clip {
            let clip = to_embedded_rect(clip);
            let mut clipped = target.clipped(&clip);
            draw_box_to(&mut clipped, bounds, paint)
        } else {
            draw_box_to(&mut target, bounds, paint)
        };

        result.map_err(EmbeddedGraphicsError::Target)
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

        let (font_id, font) = self.resolve_font(style.font);
        // snapshot only the borrowed font references.
        // the shaper reas this while the visitor is free to mutate the glyph bitmap
        // cache in FontResources
        let registry = self.font_resources.registry();

        draw_text_to(
            self.target,
            text,
            bounds,
            text_clip,
            &registry,
            self.font_resources,
            font_id,
            font,
            style,
            self.coverage_mode,
        )
    }

    fn draw_image(
        &mut self,
        source: ImageSource,
        bounds: Rect,
        fit: ImageFit,
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

        image
            .draw(self.target, bounds, fit, Some(image_clip))
            .map_err(EmbeddedGraphicsError::Target)
    }

    fn draw_canvas(
        &mut self,
        bounds: Rect,
        clip: Option<Rect>,
        draw: &mut dyn FnMut(Rect, &mut dyn CanvasPainter),
    ) -> Result<(), Self::Error> {
        if bounds.width().is_non_positive() || bounds.height().is_non_positive() {
            return Ok(());
        }

        let Some(canvas_clip) = (match clip {
            Some(clip) => clip.intersection(bounds),
            None => Some(bounds),
        }) else {
            return Ok(());
        };

        let mut target = self.target.color_converted::<EgRgb888>();
        let clip = to_embedded_rect(canvas_clip);
        let mut clipped = target.clipped(&clip);

        let local_bounds = Rect::new(Point::ZERO, bounds.size);
        let mut canvas_painter = EmbeddedGraphicsCanvasPainter::new(&mut clipped, bounds.origin);

        draw(local_bounds, &mut canvas_painter);

        canvas_painter
            .finish()
            .map_err(EmbeddedGraphicsError::Target)
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

#[allow(clippy::too_many_arguments)]
fn draw_text_to<D, const FONTS: usize, const GLYPH_SLOTS: usize>(
    target: &mut D,
    text: &str,
    bounds: Rect,
    clip: Rect,
    registry: &FontRegistry<'_, FONTS>,
    font_resources: &mut FontResources<'_, '_, FONTS, GLYPH_SLOTS>,
    font_id: FontId,
    font: &dyn FontFace,
    style: ResolvedTextStyle,
    coverage_mode: CoverageMode<D>,
) -> Result<(), EmbeddedGraphicsError<D::Error>>
where
    D: EgDrawTarget,
    D::Color: From<EgRgb888>,
{
    if text.is_empty() {
        return Ok(());
    }

    let size_px = font_size_px(style);
    let line_advance = text_line_advance(font, size_px, style);
    let baseline_offset = font.metrics(size_px).ascent;
    let color = to_rgb888(style.color);
    let shaper = SimpleShaper::new();

    let mut y = bounds.origin.y;
    let mut error = None;

    for_each_visible_text_line(
        text,
        style.wrap,
        bounds.width(),
        style.max_lines,
        style.overflow,
        |line| measure_shaped_line(registry, font_id, size_px, line),
        |line| measure_shaped_line_with_ellipsis(registry, font_id, size_px, line),
        |line| {
            if error.is_some() {
                return;
            }

            let mut glyphs = [ShapedGlyph::EMPTY; SHAPED_LINE_GLYPH_CAPACITY];
            let mut shape_state = ShapeState::new();

            let text_summary = match shaper.shape_piece_into(
                registry,
                font_id,
                size_px,
                line.text,
                &mut shape_state,
                &mut glyphs,
            ) {
                Ok(summary) => summary,
                Err(shape_error) => {
                    error = Some(EmbeddedGraphicsError::Shape(shape_error));
                    return;
                }
            };

            let text_glyph_count = text_summary.glyph_count();
            let mut glyph_count = text_glyph_count;
            let mut advance = text_summary.advance();

            if line.ellipsis {
                let ellipsis_summary = match shaper.shape_piece_into(
                    registry,
                    font_id,
                    size_px,
                    ELLIPSIS,
                    &mut shape_state,
                    &mut glyphs[glyph_count..],
                ) {
                    Ok(summary) => summary,
                    Err(shape_error) => {
                        error = Some(EmbeddedGraphicsError::Shape(shape_error));
                        return;
                    }
                };

                glyph_count = glyph_count.saturating_add(ellipsis_summary.glyph_count());
                advance += ellipsis_summary.advance();
            }

            let run = match shaper.visual_order(
                line.text,
                text_glyph_count,
                &mut glyphs[..glyph_count],
                advance,
            ) {
                Ok(run) => run,
                Err(shape_error) => {
                    error = Some(EmbeddedGraphicsError::Shape(shape_error));
                    return;
                }
            };

            debug_assert_eq!(run.advance(), line.width);

            let mut pen_x = aligned_line_x(bounds, line.width, style.align, run.direction());
            let baseline = y + baseline_offset;

            if let Err(draw_error) = draw_shaped_run(
                target,
                font_resources,
                size_px,
                &run,
                baseline,
                color,
                clip,
                coverage_mode,
                &mut pen_x,
            ) {
                error = Some(draw_error);
                return;
            }

            y += line_advance;
        },
    );

    match error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_shaped_run<D, const FONTS: usize, const GLYPH_SLOTS: usize>(
    target: &mut D,
    font_resources: &mut FontResources<'_, '_, FONTS, GLYPH_SLOTS>,
    size_px: u16,
    run: &ShapedRun<'_>,
    baseline: Pixels,
    color: EgRgb888,
    clip: Rect,
    coverage_mode: CoverageMode<D>,
    pen_x: &mut Pixels,
) -> Result<(), EmbeddedGraphicsError<D::Error>>
where
    D: EgDrawTarget,
    D::Color: From<EgRgb888>,
{
    for shaped in run.glyphs().iter().copied() {
        let bitmap = font_resources
            .glyph_bitmap(shaped.font(), shaped.glyph(), size_px)
            .map_err(EmbeddedGraphicsError::Font)?;

        let metrics = bitmap.metrics();
        let offset = shaped.offset();

        let origin = Point::new(
            *pen_x + offset.x + metrics.bearing_x,
            baseline + offset.y + metrics.bearing_y,
        );

        draw_coverage_bitmap(target, &bitmap, origin, color, clip, coverage_mode)?;

        *pen_x += shaped.advance();
    }

    Ok(())
}

fn draw_coverage_bitmap<D>(
    target: &mut D,
    bitmap: &GlyphBitmap<'_>,
    origin: Point,
    color: EgRgb888,
    clip: Rect,
    coverage_mode: CoverageMode<D>,
) -> Result<(), EmbeddedGraphicsError<D::Error>>
where
    D: EgDrawTarget,
    D::Color: From<EgRgb888>,
{
    let width = usize::from(bitmap.width());
    let height = usize::from(bitmap.height());

    if width == 0 || height == 0 {
        return Ok(());
    }

    let coverage = bitmap.coverage();
    debug_assert_eq!(coverage.len(), width.saturating_mul(height,),);

    match coverage_mode {
        CoverageMode::BinaryThreshold => {
            let pixels = coverage
                .iter()
                .copied()
                .enumerate()
                .filter_map(|(index, coverage)| {
                    if coverage < 128 {
                        return None;
                    }

                    let point = coverage_point(origin, width, index)?;
                    if !point_in_rect(point, clip) {
                        return None;
                    }

                    Some(EgPixel(point, D::Color::from(color)))
                });

            target
                .draw_iter(pixels)
                .map_err(EmbeddedGraphicsError::Target)
        }
        CoverageMode::OrderedDither4x4 => {
            let pixels = coverage
                .iter()
                .copied()
                .enumerate()
                .filter_map(|(index, coverage)| {
                    let point = coverage_point(origin, width, index)?;
                    if !point_in_rect(point, clip) {
                        return None;
                    }

                    if !ordered_dither_accepts(coverage, point) {
                        return None;
                    }

                    Some(EgPixel(point, D::Color::from(color)))
                });

            target
                .draw_iter(pixels)
                .map_err(EmbeddedGraphicsError::Target)
        }
        CoverageMode::AlphaBlend { read_pixel } => {
            // fully covered pixels need no destination readback. Keep those together in
            // one DrawTarget call. Only edge pixels require the slower read/blend/write path
            let solid_pixels =
                coverage
                    .iter()
                    .copied()
                    .enumerate()
                    .filter_map(|(index, coverage)| {
                        if coverage != 255 {
                            return None;
                        }

                        let point = coverage_point(origin, width, index)?;
                        if !point_in_rect(point, clip) {
                            return None;
                        }

                        Some(EgPixel(point, D::Color::from(color)))
                    });

            target
                .draw_iter(solid_pixels)
                .map_err(EmbeddedGraphicsError::Target)?;

            for (index, coverage) in coverage.iter().copied().enumerate() {
                if coverage == 0 || coverage == 255 {
                    continue;
                }

                let Some(point) = coverage_point(origin, width, index) else {
                    continue;
                };

                if !point_in_rect(point, clip) {
                    continue;
                }

                let Some(background) = read_pixel(&*target, point) else {
                    continue;
                };

                let blended = alpha_blend_rgb888(color, background, coverage);

                target
                    .draw_iter(core::iter::once(EgPixel(point, D::Color::from(blended))))
                    .map_err(EmbeddedGraphicsError::Target)?;
            }

            Ok(())
        }
    }
}

fn measure_shaped_line<const FONTS: usize>(
    registry: &FontRegistry<'_, FONTS>,
    font: FontId,
    size_px: u16,
    text: &str,
) -> Pixels {
    SimpleShaper::new()
        .measure(registry, font, size_px, text)
        .advance()
}

fn measure_shaped_line_with_ellipsis<const FONTS: usize>(
    registry: &FontRegistry<'_, FONTS>,
    font: FontId,
    size_px: u16,
    text: &str,
) -> Pixels {
    let shaper = SimpleShaper::new();
    let mut state = ShapeState::new();
    let text = shaper.shape_piece_with(registry, font, size_px, text, &mut state, |_| {});
    let ellipsis = shaper.shape_piece_with(registry, font, size_px, ELLIPSIS, &mut state, |_| {});

    text.advance() + ellipsis.advance()
}

fn text_line_advance(font: &dyn FontFace, size_px: u16, style: ResolvedTextStyle) -> Pixels {
    match style.line_height {
        LineHeight::Normal => font.metrics(size_px).line_height().non_negative(),
        LineHeight::Pixels(height) => height.non_negative(),
    }
}

fn aligned_line_x(
    bounds: Rect,
    line_width: Pixels,
    align: TextAlign,
    direction: TextDirection,
) -> Pixels {
    use TextAlign::*;
    use TextDirection::*;

    let remaining = (bounds.width() - line_width).non_negative();

    match (align, direction) {
        (Center, _) => bounds.origin.x + remaining / 2,
        (Start, LeftToRight) | (End, RightToLeft) => bounds.origin.x,
        (End, LeftToRight) | (Start, RightToLeft) => bounds.origin.x + remaining,
    }
}

fn font_size_px(style: ResolvedTextStyle) -> u16 {
    let size = style.font_size.max(px(1)).get();
    u16::try_from(size).unwrap_or(u16::MAX)
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

unsafe fn draw_erased_scaled_image<T, D>(
    image: *const (),
    target: &mut D,
    destination: Rect,
    clip: Option<Rect>,
) -> Result<(), D::Error>
where
    D: EgDrawTarget,
    T: EgImageDrawable + EgGetPixel<Color = <T as EgImageDrawable>::Color>,
    <T as EgImageDrawable>::Color: Into<D::Color>,
{
    let image = unsafe { &*image.cast::<T>() };
    let source_size = image.size();
    if source_size.width == 0
        || source_size.height == 0
        || destination.width().is_non_positive()
        || destination.height().is_non_positive()
    {
        return Ok(());
    }

    let Some(visible) = (match clip {
        Some(clip) => destination.intersection(clip),
        None => Some(destination),
    }) else {
        return Ok(());
    };

    let destination_x = destination.x().get();
    let destination_y = destination.y().get();
    let destination_width = destination.width().get().max(1);
    let destination_height = destination.height().get().max(1);

    let left = visible.x().get();
    let top = visible.y().get();
    let right = visible.right().get();
    let bottom = visible.bottom().get();

    let source_width = u64::from(source_size.width);
    let source_height = u64::from(source_size.height);

    let pixels = (top..bottom).flat_map(|y| {
        (left..right).filter_map(move |x| {
            let relative_x = i64::from(x) - i64::from(destination_x);
            let relative_y = i64::from(y) - i64::from(destination_y);
            if relative_x < 0 || relative_y < 0 {
                return None;
            }

            let source_x = (u64::try_from(relative_x)
                .unwrap_or(0)
                .saturating_mul(source_width)
                / u64::try_from(destination_width).unwrap_or(1))
            .min(source_width.saturating_sub(1));

            let source_y = (u64::try_from(relative_y)
                .unwrap_or(0)
                .saturating_mul(source_height)
                / u64::try_from(destination_height).unwrap_or(1))
            .min(source_height.saturating_sub(1));

            let source_point = EgPoint::new(
                i32::try_from(source_x).unwrap_or(i32::MAX),
                i32::try_from(source_y).unwrap_or(i32::MAX),
            );

            image
                .pixel(source_point)
                .map(|color| EgPixel(EgPoint::new(x, y), color))
        })
    });

    let mut target = target.color_converted::<<T as EgImageDrawable>::Color>();

    target.draw_iter(pixels)
}

fn coverage_point(origin: Point, width: usize, index: usize) -> Option<EgPoint> {
    if width == 0 {
        return None;
    }

    let x = index % width;
    let x = i32::try_from(x).ok()?;
    let y = index / width;
    let y = i32::try_from(y).ok()?;

    Some(EgPoint::new(
        origin.x.get().saturating_add(x),
        origin.y.get().saturating_add(y),
    ))
}

fn point_in_rect(point: EgPoint, rect: Rect) -> bool {
    point.x >= rect.x().get()
        && point.y >= rect.y().get()
        && point.x < rect.right().get()
        && point.y < rect.bottom().get()
}

fn alpha_blend_rgb888(foreground: EgRgb888, background: EgRgb888, coverage: u8) -> EgRgb888 {
    EgRgb888::new(
        alpha_blend_channel(foreground.r(), background.r(), coverage),
        alpha_blend_channel(foreground.g(), background.g(), coverage),
        alpha_blend_channel(foreground.b(), background.b(), coverage),
    )
}

fn alpha_blend_channel(foreground: u8, background: u8, coverage: u8) -> u8 {
    let alpha = u32::from(coverage);
    let inverse = 255u32.saturating_sub(alpha);
    let value = u32::from(foreground)
        .saturating_mul(alpha)
        .saturating_add(u32::from(background).saturating_mul(inverse))
        .saturating_add(127)
        / 255;

    u8::try_from(value).unwrap_or(u8::MAX)
}

const BAYER_4X4: [u8; 16] = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5];

fn ordered_dither_accepts(coverage: u8, point: EgPoint) -> bool {
    if coverage == 0 {
        return false;
    }

    if coverage == 255 {
        return true;
    }

    let x = point.x.rem_euclid(4) as usize;
    let y = point.y.rem_euclid(4) as usize;
    let rank = BAYER_4X4[y * 4 + x];
    let threshold = rank.saturating_mul(16).saturating_add(8);

    coverage > threshold
}

#[cfg(test)]
mod tests {
    use embedded_graphics::{
        Pixel,
        draw_target::DrawTarget as EgDrawTarget,
        geometry::{OriginDimensions, Point as EgPoint, Size as EgSize},
        image::{GetPixel as EgGetPixel, ImageDrawable as EgImageDrawable},
        mock_display::MockDisplay,
        mono_font::ascii::FONT_6X10,
        pixelcolor::Rgb888,
        primitives::Rectangle as EgRectangle,
    };

    use crate::{
        backend::{
            EmbeddedGraphicsPainter, MonoFontFace,
            embedded_graphics::{EmbeddedGraphicsImage, aligned_line_x},
        },
        *,
    };

    static TEST_FONT_FACE: MonoFontFace<'static> = MonoFontFace::new(&FONT_6X10);

    fn test_font_resources(storage: &mut [u8]) -> FontResources<'static, '_, 1, 64> {
        let mut resources = FontResources::new(storage);

        let id = resources.register(&TEST_FONT_FACE).unwrap();
        assert_eq!(id, FontId::DEFAULT,);

        resources
    }

    #[test]
    fn embedded_graphics_backend_rasterizes_box() {
        let mut display = MockDisplay::<Rgb888>::new();

        {
            let mut glyph_storage = [0; 4096];
            let mut fonts = test_font_resources(&mut glyph_storage);
            let mut painter = EmbeddedGraphicsPainter::new(&mut display, &mut fonts, []);

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

        let mut glyph_storage = [0; 4096];
        let mut fonts = test_font_resources(&mut glyph_storage);
        let painter = EmbeddedGraphicsPainter::new(&mut display, &mut fonts, []);

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

        let mut glyph_storage = [0; 4096];
        let mut fonts = test_font_resources(&mut glyph_storage);
        let painter = EmbeddedGraphicsPainter::new(&mut display, &mut fonts, []);

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

        assert_eq!(
            aligned_line_x(
                bounds,
                px(40),
                TextAlign::Center,
                TextDirection::LeftToRight
            ),
            px(40)
        );
        assert_eq!(
            aligned_line_x(bounds, px(40), TextAlign::End, TextDirection::LeftToRight),
            px(70)
        );
        assert_eq!(
            aligned_line_x(bounds, px(40), TextAlign::Start, TextDirection::LeftToRight),
            px(10)
        );
    }

    #[test]
    fn max_lines_limits_measured_height() {
        let mut display = MockDisplay::<Rgb888>::new();

        let mut glyph_storage = [0; 4096];
        let mut fonts = test_font_resources(&mut glyph_storage);
        let painter = EmbeddedGraphicsPainter::new(&mut display, &mut fonts, []);

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

        let mut glyph_storage = [0; 4096];
        let mut fonts = test_font_resources(&mut glyph_storage);
        let painter = EmbeddedGraphicsPainter::new(&mut display, &mut fonts, []);

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

        let mut glyph_storage = [0; 4096];
        let mut fonts = test_font_resources(&mut glyph_storage);
        let painter = EmbeddedGraphicsPainter::new(&mut display, &mut fonts, []);

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

    impl EgGetPixel for SolidTestImage {
        type Color = Rgb888;

        fn pixel(&self, point: EgPoint) -> Option<Self::Color> {
            if point.x < 0 || point.y < 0 {
                return None;
            }

            let x = u32::try_from(point.x).ok()?;
            let y = u32::try_from(point.y).ok()?;
            if x >= self.size.width || y >= self.size.height {
                return None;
            }

            Some(self.color)
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
            let mut glyph_storage = [0; 4096];
            let mut fonts = test_font_resources(&mut glyph_storage);
            let mut painter = EmbeddedGraphicsPainter::new(&mut display, &mut fonts, [registered]);

            painter
                .draw_image(
                    source,
                    Rect::new(Point::new(px(2), px(3)), source.size()),
                    ImageFit::None,
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
            let mut glyph_storage = [0; 4096];
            let mut fonts = test_font_resources(&mut glyph_storage);
            let mut painter = EmbeddedGraphicsPainter::new(&mut display, &mut fonts, [registered]);

            painter
                .draw_image(
                    source,
                    Rect::new(Point::new(px(10), px(10)), Size::new(px(2), px(2))),
                    ImageFit::None,
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
            let mut glyph_storage = [0; 4096];
            let mut fonts = test_font_resources(&mut glyph_storage);
            let mut painter = EmbeddedGraphicsPainter::new(&mut display, &mut fonts, [registered]);

            painter
                .draw_image(
                    source,
                    Rect::new(Point::new(px(4), px(4)), source.size()),
                    ImageFit::None,
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

    #[test]
    fn embedded_graphics_backend_scales_image_with_contain() {
        let bitmap = SolidTestImage {
            size: EgSize::new(4, 2),
            color: Rgb888::new(255, 0, 0),
        };

        let registered = EmbeddedGraphicsImage::new(&bitmap);
        let source = registered.source(ImageId::new(0));

        let mut display = MockDisplay::<Rgb888>::new();

        {
            let mut glyph_storage = [0; 4096];
            let mut fonts = test_font_resources(&mut glyph_storage);
            let mut painter = EmbeddedGraphicsPainter::new(&mut display, &mut fonts, [registered]);

            painter
                .draw_image(
                    source,
                    Rect::new(Point::ZERO, Size::new(px(8), px(8))),
                    ImageFit::Contain,
                    None,
                )
                .unwrap();
        }

        assert_eq!(
            display.get_pixel(EgPoint::new(0, 2,)),
            Some(Rgb888::new(255, 0, 0,))
        );
        assert_eq!(
            display.get_pixel(EgPoint::new(7, 5,)),
            Some(Rgb888::new(255, 0, 0,))
        );
        assert_eq!(display.get_pixel(EgPoint::new(0, 1,)), None);
        assert_eq!(display.get_pixel(EgPoint::new(0, 6,)), None);
    }

    fn draw_backend_test_canvas(bounds: Rect, painter: &mut dyn CanvasPainter) {
        painter.fill_rect(bounds, Color::RED);
        painter.fill_circle(Point::new(px(2), px(2)), px(1), Color::BLUE);
    }

    fn draw_oversized_canvas(_bounds: Rect, painter: &mut dyn CanvasPainter) {
        painter.fill_rect(
            Rect::new(Point::new(px(-10), px(-10)), Size::new(px(40), px(40))),
            Color::GREEN,
        );
    }

    #[test]
    fn embedded_graphics_backend_draws_canvas_at_visual_origin() {
        let mut display = MockDisplay::<Rgb888>::new();
        display.set_allow_overdraw(true);

        {
            let mut glyph_storage = [0; 4096];
            let mut fonts = test_font_resources(&mut glyph_storage);
            let mut painter = EmbeddedGraphicsPainter::new(&mut display, &mut fonts, []);

            painter
                .draw_canvas(
                    Rect::new(Point::new(px(10), px(20)), Size::new(px(8), px(6))),
                    None,
                    &mut draw_backend_test_canvas,
                )
                .unwrap();
        }

        assert_eq!(
            display.get_pixel(EgPoint::new(10, 20,)),
            Some(Rgb888::new(255, 0, 0,))
        );
        assert_eq!(
            display.get_pixel(EgPoint::new(12, 22,)),
            Some(Rgb888::new(0, 0, 255,))
        );
        assert_eq!(
            display.get_pixel(EgPoint::new(17, 25,)),
            Some(Rgb888::new(255, 0, 0,))
        );
        assert_eq!(display.get_pixel(EgPoint::new(9, 20,)), None);
    }

    #[test]
    fn embedded_graphics_backend_clips_custom_drawing_to_canvas_bounds() {
        let mut display = MockDisplay::<Rgb888>::new();

        {
            let mut glyph_storage = [0; 4096];
            let mut fonts = test_font_resources(&mut glyph_storage);
            let mut painter = EmbeddedGraphicsPainter::new(&mut display, &mut fonts, []);

            painter
                .draw_canvas(
                    Rect::new(Point::new(px(5), px(5)), Size::new(px(4), px(4))),
                    None,
                    &mut draw_oversized_canvas,
                )
                .unwrap();
        }

        assert_eq!(
            display.get_pixel(EgPoint::new(5, 5,)),
            Some(Rgb888::new(0, 255, 0,))
        );
        assert_eq!(
            display.get_pixel(EgPoint::new(8, 8,)),
            Some(Rgb888::new(0, 255, 0,))
        );
        assert_eq!(display.get_pixel(EgPoint::new(4, 5,)), None);
        assert_eq!(display.get_pixel(EgPoint::new(9, 5,)), None);
    }

    #[test]
    fn embedded_graphics_backend_combines_canvas_and_ancestor_clipping() {
        let mut display = MockDisplay::<Rgb888>::new();

        {
            let mut glyph_storage = [0; 4096];
            let mut fonts = test_font_resources(&mut glyph_storage);
            let mut painter = EmbeddedGraphicsPainter::new(&mut display, &mut fonts, []);

            painter
                .draw_canvas(
                    Rect::new(Point::new(px(5), px(5)), Size::new(px(10), px(10))),
                    Some(Rect::new(Point::new(px(8), px(8)), Size::new(px(3), px(3)))),
                    &mut draw_oversized_canvas,
                )
                .unwrap();
        }

        assert_eq!(
            display.get_pixel(EgPoint::new(8, 8,)),
            Some(Rgb888::new(0, 255, 0,))
        );
        assert_eq!(
            display.get_pixel(EgPoint::new(10, 10,)),
            Some(Rgb888::new(0, 255, 0,))
        );
        assert_eq!(display.get_pixel(EgPoint::new(7, 8,)), None);
        assert_eq!(display.get_pixel(EgPoint::new(11, 8,)), None);
    }

    #[test]
    fn embedded_graphics_backend_clears_only_partial_damage() {
        let mut display = MockDisplay::<Rgb888>::new();

        display.set_allow_overdraw(true);

        display
            .draw_iter([
                Pixel(EgPoint::new(2, 2), Rgb888::new(255, 0, 0)),
                Pixel(EgPoint::new(8, 8), Rgb888::new(255, 0, 0)),
            ])
            .unwrap();

        {
            let mut glyph_storage = [0; 4096];
            let mut fonts = test_font_resources(&mut glyph_storage);
            let mut painter = EmbeddedGraphicsPainter::new(&mut display, &mut fonts, []);

            painter
                .clear_damage(
                    DamageRegion::from_rect(Rect::new(
                        Point::new(px(0), px(0)),
                        Size::new(px(5), px(5)),
                    )),
                    Color::BLACK,
                )
                .unwrap();
        }

        // inside damage was restored to the clear color.
        assert_eq!(
            display.get_pixel(EgPoint::new(2, 2),),
            Some(Rgb888::new(0, 0, 0),),
        );
        // pixels outside damage were untouched.
        assert_eq!(
            display.get_pixel(EgPoint::new(8, 8),),
            Some(Rgb888::new(255, 0, 0),),
        );
    }

    #[test]
    fn embedded_graphics_backend_full_damage_clears_target() {
        let mut display = MockDisplay::<Rgb888>::new();

        display.set_allow_overdraw(true);

        display
            .draw_iter([
                Pixel(EgPoint::new(2, 2), Rgb888::new(255, 0, 0)),
                Pixel(EgPoint::new(8, 8), Rgb888::new(0, 255, 0)),
            ])
            .unwrap();

        {
            let mut glyph_storage = [0; 4096];
            let mut fonts = test_font_resources(&mut glyph_storage);
            let mut painter = EmbeddedGraphicsPainter::new(&mut display, &mut fonts, []);

            painter
                .clear_damage(DamageRegion::full(), Color::BLUE)
                .unwrap();
        }

        assert_eq!(
            display.get_pixel(EgPoint::new(2, 2),),
            Some(Rgb888::new(0, 0, 255),),
        );
        assert_eq!(
            display.get_pixel(EgPoint::new(8, 8),),
            Some(Rgb888::new(0, 0, 255),),
        );
    }

    #[test]
    fn embedded_graphics_backend_draws_text_through_font_resources() {
        let mut display = MockDisplay::<Rgb888>::new();
        let mut glyph_storage = [0u8; 4096];
        let mut fonts = test_font_resources(&mut glyph_storage);

        {
            let mut painter = EmbeddedGraphicsPainter::new(&mut display, &mut fonts, []);

            painter
                .draw_text(
                    "A",
                    Rect::new(Point::ZERO, Size::new(px(20), px(20))),
                    ResolvedTextStyle::default(),
                    None,
                )
                .unwrap();
        }

        assert!(fonts.glyph_cache_used_bytes() > 0,);

        let has_black_pixel = (0..10).any(|y| {
            (0..6).any(|x| display.get_pixel(EgPoint::new(x, y)) == Some(Rgb888::new(0, 0, 0)))
        });

        assert!(has_black_pixel,);
    }

    #[test]
    fn alpha_blending_preserves_coverage_endpoints() {
        let black = Rgb888::new(0, 0, 0);
        let white = Rgb888::new(255, 255, 255);

        assert_eq!(super::alpha_blend_rgb888(black, white, 0,), white,);
        assert_eq!(super::alpha_blend_rgb888(black, white, 255,), black,);
    }

    #[test]
    fn alpha_blending_produces_intermediate_gray() {
        let black = Rgb888::new(0, 0, 0);
        let white = Rgb888::new(255, 255, 255);

        assert_eq!(
            super::alpha_blend_rgb888(black, white, 128,),
            Rgb888::new(127, 127, 127,),
        );
    }

    #[test]
    fn ordered_dither_half_coverage_draws_half_of_matrix() {
        let mut drawn = 0usize;
        for y in 0..4 {
            for x in 0..4 {
                if super::ordered_dither_accepts(128, EgPoint::new(x, y)) {
                    drawn += 1;
                }
            }
        }

        assert_eq!(drawn, 8,);
    }

    #[test]
    fn ordered_dither_is_stable_in_absolute_coordinates() {
        for y in 0..4 {
            for x in 0..4 {
                let first = super::ordered_dither_accepts(93, EgPoint::new(x, y));
                let repeated = super::ordered_dither_accepts(93, EgPoint::new(x + 4, y + 4));

                assert_eq!(first, repeated,);
            }
        }
    }

    #[test]
    fn rtl_start_and_end_alignment_are_mirrored() {
        let bounds = Rect::new(Point::new(px(10), px(5)), Size::new(px(100), px(20)));

        assert_eq!(
            aligned_line_x(bounds, px(40), TextAlign::Start, TextDirection::RightToLeft,),
            px(70),
        );
        assert_eq!(
            aligned_line_x(
                bounds,
                px(40),
                TextAlign::Center,
                TextDirection::RightToLeft,
            ),
            px(40),
        );
        assert_eq!(
            aligned_line_x(bounds, px(40), TextAlign::End, TextDirection::RightToLeft,),
            px(10),
        );
    }
}
