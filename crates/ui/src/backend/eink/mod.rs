use embedded_graphics::{
    Drawable,
    draw_target::DrawTargetExt,
    geometry::{Point as EgPoint, Size as EgSize},
    prelude::DrawTarget as EgDrawTarget,
    primitives::{
        Primitive, PrimitiveStyleBuilder as EgPrimitiveStyleBuilder, Rectangle as EgRectangle,
        RoundedRectangle as EgRoundedRectangle, StrokeAlignment as EgStrokeAlignment,
    },
};

use crate::{
    BoxPaint, CanvasPainter, Color, DamageRegion, GlyphCacheError, ImagePaint, ImageSource,
    Painter, Point, Rect, ResolvedTextStyle, ResourcePainter, ShapeError, Size,
    backend::eink::tone::{BinaryDitherTarget, gray2_tone},
    px,
    resources::RuntimeResources,
};

mod canvas;
mod coverage;
mod image;
mod text;
mod tone;

pub use coverage::EInkCoverageMode;
pub use tone::{EInkPaintReport, EInkTone, EInkUiMode};

pub use embedded_graphics::pixelcolor::Gray2;

use canvas::EInkCanvasPainter;
use coverage::color_to_gray2;
use image::draw_image_to;
use text::draw_text_to;

#[derive(Debug)]
pub enum EInkError<E> {
    Target(E),
    Font(GlyphCacheError),
    Shape(ShapeError),
}

pub struct EInkPainter<'target, D>
where
    D: EgDrawTarget<Color = Gray2>,
{
    target: &'target mut D,
    coverage_mode: EInkCoverageMode<D>,
    ui_mode: EInkUiMode,
    report: EInkPaintReport,
}

impl<'target, D> EInkPainter<'target, D>
where
    D: EgDrawTarget<Color = Gray2>,
{
    pub fn new(target: &'target mut D) -> Self {
        Self {
            target,
            coverage_mode: EInkCoverageMode::BinaryThreshold,
            ui_mode: EInkUiMode::NativeGray2,
            report: EInkPaintReport::default(),
        }
    }

    pub fn with_coverage_mode(mut self, coverage_mode: EInkCoverageMode<D>) -> Self {
        self.coverage_mode = coverage_mode;
        self
    }

    pub fn with_ui_mode(mut self, ui_mode: EInkUiMode) -> Self {
        self.ui_mode = ui_mode;
        self
    }

    pub fn target_mut(&mut self) -> &mut D {
        self.target
    }

    pub const fn report(&self) -> EInkPaintReport {
        self.report
    }

    fn record_native_ui_color(&mut self, color: Color) {
        if self.ui_mode != EInkUiMode::NativeGray2 {
            return;
        }

        self.report.include(gray2_tone(color_to_gray2(color)));
    }

    pub fn clear_damage(&mut self, damage: DamageRegion, color: Color) -> Result<(), D::Error> {
        if damage.is_none() {
            return Ok(());
        }

        self.record_native_ui_color(color);
        let color = color_to_gray2(color);

        if damage.is_full() {
            return self.target.clear(color);
        }

        let target_bounds = self.target.bounding_box();

        let target_bounds = Rect::new(
            Point::new(px(target_bounds.top_left.x), px(target_bounds.top_left.y)),
            from_embedded_size(target_bounds.size),
        );

        let damage = damage.clipped_to(target_bounds);

        if damage.is_none() {
            return Ok(());
        }

        for &rect in damage.rects() {
            self.target.fill_solid(&to_embedded_rect(rect), color)?;
        }

        Ok(())
    }
}

impl<D> Painter for EInkPainter<'_, D>
where
    D: EgDrawTarget<Color = Gray2>,
{
    type Error = EInkError<D::Error>;

    fn draw_box(
        &mut self,
        bounds: Rect,
        paint: BoxPaint,
        clip: Option<Rect>,
    ) -> Result<(), Self::Error> {
        if let Some(background) = paint.background {
            self.record_native_ui_color(background);
        }

        if let Some(border) = paint.border {
            self.record_native_ui_color(border.color);
        }

        let result = match self.ui_mode {
            EInkUiMode::NativeGray2 => {
                if let Some(clip) = clip {
                    let clip = to_embedded_rect(clip);
                    let mut clipped = self.target.clipped(&clip);

                    draw_box_to(&mut clipped, bounds, paint)
                } else {
                    draw_box_to(self.target, bounds, paint)
                }
            }

            EInkUiMode::BinaryDither => {
                if let Some(clip) = clip {
                    let clip = to_embedded_rect(clip);
                    let mut clipped = self.target.clipped(&clip);
                    let mut dithered = BinaryDitherTarget::new(&mut clipped);

                    draw_box_to(&mut dithered, bounds, paint)
                } else {
                    let mut dithered = BinaryDitherTarget::new(self.target);

                    draw_box_to(&mut dithered, bounds, paint)
                }
            }
        };

        result.map_err(EInkError::Target)
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

        let target_bounds = self.target.bounding_box();

        let target_bounds = Rect::new(
            Point::new(px(target_bounds.top_left.x), px(target_bounds.top_left.y)),
            from_embedded_size(target_bounds.size),
        );

        let Some(canvas_clip) = canvas_clip.intersection(target_bounds) else {
            return Ok(());
        };

        let local_bounds = Rect::new(Point::ZERO, bounds.size);

        let coverage_mode = match self.ui_mode {
            EInkUiMode::NativeGray2 => self.coverage_mode,
            EInkUiMode::BinaryDither => EInkCoverageMode::OrderedDither4x4,
        };

        if self.ui_mode == EInkUiMode::NativeGray2 {
            // canvas callbacks can paint arbitrary colors and coverage. Conservatively
            // report Gray4 in native mode.
            self.report.include(EInkTone::Gray4);
        }

        let mut painter = EInkCanvasPainter::new(
            self.target,
            bounds.origin,
            canvas_clip,
            coverage_mode,
            self.ui_mode,
        );

        draw(local_bounds, &mut painter);

        painter.finish().map_err(EInkError::Target)
    }
}

impl<D, const FONTS: usize, const GLYPH_SLOTS: usize, const GLYPH_BYTES: usize, const IMAGES: usize>
    ResourcePainter<RuntimeResources<'_, FONTS, GLYPH_SLOTS, GLYPH_BYTES, IMAGES>>
    for EInkPainter<'_, D>
where
    D: EgDrawTarget<Color = Gray2>,
{
    fn draw_text(
        &mut self,
        resources: &mut RuntimeResources<'_, FONTS, GLYPH_SLOTS, GLYPH_BYTES, IMAGES>,
        text: &str,
        bounds: Rect,
        style: ResolvedTextStyle,
        clip: Option<Rect>,
    ) -> Result<(), Self::Error> {
        if bounds.width().is_non_positive() || bounds.height().is_non_positive() {
            return Ok(());
        }

        let Some(clip) = (match clip {
            Some(clip) => clip.intersection(bounds),
            None => Some(bounds),
        }) else {
            return Ok(());
        };

        let font = resources
            .resolve_font_family_weight(style.font_family, style.font_weight)
            .expect("EInkPainter requires a default font");

        let registry = resources.font_registry();

        let coverage_mode = match self.ui_mode {
            EInkUiMode::NativeGray2 => self.coverage_mode,
            EInkUiMode::BinaryDither => EInkCoverageMode::OrderedDither4x4,
        };

        if self.ui_mode == EInkUiMode::NativeGray2 {
            self.record_native_ui_color(style.color);

            if matches!(self.coverage_mode, EInkCoverageMode::AlphaBlend { .. }) {
                // partial glyph coverage can create intermediate Gray2 output even
                // for a binary foreground color.
                self.report.include(EInkTone::Gray4);
            }
        }

        draw_text_to(
            self.target,
            text,
            bounds,
            clip,
            &registry,
            resources,
            font,
            style,
            coverage_mode,
        )
    }

    fn draw_image(
        &mut self,
        resources: &mut RuntimeResources<'_, FONTS, GLYPH_SLOTS, GLYPH_BYTES, IMAGES>,
        source: ImageSource,
        bounds: Rect,
        paint: ImagePaint,
        clip: Option<Rect>,
    ) -> Result<(), Self::Error> {
        if bounds.width().is_non_positive() || bounds.height().is_non_positive() {
            return Ok(());
        }

        let Some(image) = resources.image(source.id()) else {
            debug_assert!(false, "image {:?} is not registered", source.id());
            return Ok(());
        };

        debug_assert_eq!(
            image.size(),
            source.size(),
            "registered image size differs from ImageSource size for {:?}",
            source.id(),
        );

        let Some(clip) = (match clip {
            Some(clip) => clip.intersection(bounds),
            None => Some(bounds),
        }) else {
            return Ok(());
        };

        let tone = draw_image_to(self.target, image, bounds, paint, Some(clip))
            .map_err(EInkError::Target)?;

        self.report.include(tone);

        Ok(())
    }
}

pub(super) fn to_embedded_rect(rect: Rect) -> EgRectangle {
    let width = u32::try_from(rect.width().non_negative().get()).unwrap_or(u32::MAX);

    let height = u32::try_from(rect.height().non_negative().get()).unwrap_or(u32::MAX);

    EgRectangle::new(
        EgPoint::new(rect.x().get(), rect.y().get()),
        EgSize::new(width, height),
    )
}

pub(super) fn to_embedded_point(point: Point) -> EgPoint {
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
    D: EgDrawTarget<Color = Gray2>,
{
    if bounds.width().is_non_positive() || bounds.height().is_non_positive() {
        return Ok(());
    }

    if paint.background.is_none() && paint.border.is_none() {
        return Ok(());
    }

    let mut style = EgPrimitiveStyleBuilder::new().stroke_alignment(EgStrokeAlignment::Inside);

    if let Some(background) = paint.background {
        style = style.fill_color(color_to_gray2(background));
    }

    if let Some(border) = paint.border {
        let width = u32::try_from(border.width.non_negative().get()).unwrap_or(u32::MAX);

        if width > 0 {
            style = style
                .stroke_color(color_to_gray2(border.color))
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
