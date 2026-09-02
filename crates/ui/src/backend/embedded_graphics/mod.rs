use embedded_graphics::{
    Drawable,
    draw_target::DrawTargetExt,
    geometry::{Point as EgPoint, Size as EgSize},
    pixelcolor::Rgb888 as EgRgb888,
    prelude::DrawTarget as EgDrawTarget,
    primitives::{
        Primitive, PrimitiveStyleBuilder as EgPrimitiveStyleBuilder, Rectangle as EgRectangle,
        RoundedRectangle as EgRoundedRectangle, StrokeAlignment as EgStrokeAlignment,
    },
};

use crate::{
    BoxPaint, CanvasPainter, Color, DamageRegion, GlyphCacheError, ImagePaint, ImageSource,
    Painter, Point, Rect, ResolvedTextStyle, ResourcePainter, ShapeError, Size,
    backend::embedded_graphics::{image::draw_image_to, text::draw_text_to},
    px,
    resources::RuntimeResources,
};

mod canvas;
mod coverage;
mod image;
mod text;

pub use coverage::CoverageMode;
pub use image::EmbeddedGraphicsImage;

use canvas::EmbeddedGraphicsCanvasPainter;

#[cfg(test)]
mod tests;

#[derive(Debug)]
pub enum EmbeddedGraphicsError<E> {
    Target(E),
    Font(GlyphCacheError),
    Shape(ShapeError),
}

pub struct EmbeddedGraphicsPainter<'target, D>
where
    D: EgDrawTarget,
{
    target: &'target mut D,
    coverage_mode: CoverageMode<D>,
}

impl<'target, D> EmbeddedGraphicsPainter<'target, D>
where
    D: EgDrawTarget,
{
    pub fn new(target: &'target mut D) -> Self {
        Self {
            target,
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

impl<D> Painter for EmbeddedGraphicsPainter<'_, D>
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

impl<D, const FONTS: usize, const GLYPH_SLOTS: usize, const GLYPH_BYTES: usize, const IMAGES: usize>
    ResourcePainter<RuntimeResources<'_, FONTS, GLYPH_SLOTS, GLYPH_BYTES, IMAGES>>
    for EmbeddedGraphicsPainter<'_, D>
where
    D: EgDrawTarget,
    D::Color: From<EgRgb888>,
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

        let Some(text_clip) = (match clip {
            Some(clip) => clip.intersection(bounds),
            None => Some(bounds),
        }) else {
            return Ok(());
        };

        let (font_id, font) = resources
            .resolve_font(style.font)
            .expect("EmbeddedGraphicsPainter requires a default font");

        let registry = resources.font_registry();

        draw_text_to(
            self.target,
            text,
            bounds,
            text_clip,
            &registry,
            resources,
            font_id,
            font,
            style,
            self.coverage_mode,
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

        let Some(image_clip) = (match clip {
            Some(clip) => clip.intersection(bounds),
            None => Some(bounds),
        }) else {
            return Ok(());
        };

        draw_image_to(self.target, image, bounds, paint, Some(image_clip))
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
