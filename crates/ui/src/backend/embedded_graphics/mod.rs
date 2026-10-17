use embedded_graphics::{
    Drawable, Pixel as EgPixel,
    draw_target::DrawTargetExt,
    geometry::Point as EgPoint,
    pixelcolor::Rgb888 as EgRgb888,
    prelude::DrawTarget as EgDrawTarget,
    primitives::{Primitive, PrimitiveStyle as EgPrimitiveStyle},
};

use crate::{
    BoxPaint, CanvasPainter, Color, DamageRegion, GlyphBitmap, Painter, Point, Rect,
    backend::{
        embedded::{draw_box_to, from_embedded_size, to_embedded_point, to_embedded_rect},
        embedded_graphics::coverage::draw_coverage_bitmap,
    },
    px,
};

mod canvas;
mod coverage;
mod image;

pub use coverage::CoverageMode;
pub use image::EmbeddedGraphicsImage;

use canvas::EmbeddedGraphicsCanvasPainter;

#[cfg(test)]
mod tests;

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
            draw_box_to(&mut clipped, bounds, paint, to_rgb888)
        } else {
            draw_box_to(&mut target, bounds, paint, to_rgb888)
        }
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
        let coverage_mode = self.coverage_mode;

        let mut canvas_painter = EmbeddedGraphicsCanvasPainter::new(
            self.target,
            bounds.origin,
            canvas_clip,
            coverage_mode,
        );

        draw(local_bounds, &mut canvas_painter);

        canvas_painter.finish()
    }

    fn draw_glyph(
        &mut self,
        bitmap: &GlyphBitmap<'_>,
        origin: Point,
        color: Color,
        clip: Rect,
    ) -> Result<(), Self::Error> {
        draw_coverage_bitmap(
            self.target,
            bitmap,
            origin,
            to_rgb888(color),
            clip,
            self.coverage_mode,
        )
    }

    fn fill_rect(&mut self, rect: Rect, color: Color, clip: Rect) -> Result<(), Self::Error> {
        let mut target = self.target.color_converted::<EgRgb888>();

        to_embedded_rect(rect)
            .into_styled(EgPrimitiveStyle::with_fill(to_rgb888(color)))
            .draw(&mut target.clipped(&to_embedded_rect(clip)))
    }

    fn draw_pixels(&mut self, origin: Point, pixels: &[Color]) -> Result<(), Self::Error> {
        let y = origin.y.get();

        let pixels = pixels
            .iter()
            .zip(origin.x.get()..)
            .map(|(&color, x)| EgPixel(EgPoint::new(x, y), to_rgb888(color)));

        self.target.color_converted::<EgRgb888>().draw_iter(pixels)
    }
}

fn to_rgb888(color: Color) -> EgRgb888 {
    EgRgb888::new(color.r(), color.g(), color.b())
}
