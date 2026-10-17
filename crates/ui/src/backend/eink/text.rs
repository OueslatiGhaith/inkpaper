use embedded_graphics::{
    Drawable,
    draw_target::DrawTargetExt,
    prelude::DrawTarget as EgDrawTarget,
    primitives::{Primitive, PrimitiveStyle as EgPrimitiveStyle},
};

use crate::{
    Color, GlyphBitmap, Point, Rect,
    backend::EInkPaintReport,
    rendering::text_paint::{GlyphSink, TextPaintError},
};

use super::{
    EInkCoverageMode, EInkError, Gray2,
    coverage::{color_to_gray2, draw_coverage_bitmap},
    to_embedded_rect,
};

pub(super) struct EInkGlyphSink<'a, D>
where
    D: EgDrawTarget<Color = Gray2>,
{
    pub(super) target: &'a mut D,
    pub(super) report: &'a mut EInkPaintReport,
    pub(super) coverage_mode: EInkCoverageMode<D>,
}

impl<D> GlyphSink for EInkGlyphSink<'_, D>
where
    D: EgDrawTarget<Color = Gray2>,
{
    type Error = EInkError<D::Error>;

    fn draw_glyph(
        &mut self,
        bitmap: &GlyphBitmap<'_>,
        origin: Point,
        color: Color,
        clip: Rect,
    ) -> Result<(), Self::Error> {
        draw_coverage_bitmap(
            self.target,
            self.report,
            bitmap,
            origin,
            color,
            clip,
            self.coverage_mode,
        )
    }

    fn fill_rect(&mut self, rect: Rect, color: Color, clip: Rect) -> Result<(), Self::Error> {
        to_embedded_rect(rect)
            .into_styled(EgPrimitiveStyle::with_fill(color_to_gray2(color)))
            .draw(&mut self.target.clipped(&to_embedded_rect(clip)))
            .map_err(EInkError::Target)
    }
}

pub(super) fn text_error<E>(error: TextPaintError<EInkError<E>>) -> EInkError<E> {
    match error {
        TextPaintError::Sink(error) => error,
        TextPaintError::Font(error) => EInkError::Font(error),
        TextPaintError::Shape(error) => EInkError::Shape(error),
    }
}
