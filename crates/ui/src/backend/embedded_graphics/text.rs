use embedded_graphics::{
    Drawable,
    draw_target::DrawTargetExt,
    pixelcolor::Rgb888 as EgRgb888,
    prelude::DrawTarget as EgDrawTarget,
    primitives::{Primitive, PrimitiveStyle as EgPrimitiveStyle},
};

use crate::{
    Color, GlyphBitmap, Point, Rect,
    rendering::text_paint::{GlyphSink, TextPaintError},
};

use super::{
    CoverageMode, EmbeddedGraphicsError, coverage::draw_coverage_bitmap, to_embedded_rect,
    to_rgb888,
};

pub(super) struct EmbeddedGraphicsGlyphSink<'a, D>
where
    D: EgDrawTarget,
{
    pub(super) target: &'a mut D,
    pub(super) coverage_mode: CoverageMode<D>,
}

impl<D> GlyphSink for EmbeddedGraphicsGlyphSink<'_, D>
where
    D: EgDrawTarget,
    D::Color: From<EgRgb888>,
{
    type Error = EmbeddedGraphicsError<D::Error>;

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
            .map_err(EmbeddedGraphicsError::Target)
    }
}

pub(super) fn text_error<E>(
    error: TextPaintError<EmbeddedGraphicsError<E>>,
) -> EmbeddedGraphicsError<E> {
    match error {
        TextPaintError::Sink(error) => error,
        TextPaintError::Font(error) => EmbeddedGraphicsError::Font(error),
        TextPaintError::Shape(error) => EmbeddedGraphicsError::Shape(error),
    }
}
