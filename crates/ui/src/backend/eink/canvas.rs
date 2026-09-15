use embedded_graphics::{
    Drawable, Pixel as EgPixel,
    draw_target::DrawTargetExt,
    prelude::DrawTarget as EgDrawTarget,
    primitives::{
        Circle as EgCircle, Line as EgLine, Primitive, PrimitiveStyle as EgPrimitiveStyle,
        PrimitiveStyleBuilder as EgPrimitiveStyleBuilder, StrokeAlignment as EgStrokeAlignment,
    },
};

use crate::{
    CanvasPainter, Color, Pixels, Point, Rect,
    backend::eink::{BinaryDitherTarget, EInkUiMode, tone::binary_dither_gray2},
};

use super::{
    EInkCoverageMode, Gray2,
    coverage::{alpha_blend_gray2, color_to_gray2, ordered_dither_accepts},
    to_embedded_point, to_embedded_rect,
};

pub(super) struct EInkCanvasPainter<'target, D>
where
    D: EgDrawTarget<Color = Gray2>,
{
    target: &'target mut D,
    origin: Point,
    clip: Rect,
    coverage_mode: EInkCoverageMode<D>,
    ui_mode: EInkUiMode,
    error: Option<D::Error>,
}

impl<'target, D> EInkCanvasPainter<'target, D>
where
    D: EgDrawTarget<Color = Gray2>,
{
    pub(super) fn new(
        target: &'target mut D,
        origin: Point,
        clip: Rect,
        coverage_mode: EInkCoverageMode<D>,
        ui_mode: EInkUiMode,
    ) -> Self {
        Self {
            target,
            origin,
            clip,
            coverage_mode,
            ui_mode,
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

    pub(super) fn finish(self) -> Result<(), D::Error> {
        match self.error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

impl<D> CanvasPainter for EInkCanvasPainter<'_, D>
where
    D: EgDrawTarget<Color = Gray2>,
{
    fn fill_rect(&mut self, rect: Rect, color: Color) {
        if self.error.is_some() || rect.width().is_non_positive() || rect.height().is_non_positive()
        {
            return;
        }

        let rect = self.translated_rect(rect);
        let clip = to_embedded_rect(self.clip);

        let style = EgPrimitiveStyle::with_fill(color_to_gray2(color));

        let result = {
            let mut clipped = self.target.clipped(&clip);

            match self.ui_mode {
                EInkUiMode::NativeGray2 => {
                    to_embedded_rect(rect).into_styled(style).draw(&mut clipped)
                }

                EInkUiMode::BinaryDither => {
                    let mut dithered = BinaryDitherTarget::new(&mut clipped);

                    to_embedded_rect(rect)
                        .into_styled(style)
                        .draw(&mut dithered)
                }
            }
        };

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

        let clip = to_embedded_rect(self.clip);

        let style = EgPrimitiveStyleBuilder::new()
            .stroke_color(color_to_gray2(color))
            .stroke_width(width)
            .stroke_alignment(EgStrokeAlignment::Inside)
            .build();

        let result = {
            let mut clipped = self.target.clipped(&clip);

            match self.ui_mode {
                EInkUiMode::NativeGray2 => {
                    to_embedded_rect(rect).into_styled(style).draw(&mut clipped)
                }
                EInkUiMode::BinaryDither => {
                    let mut dithered = BinaryDitherTarget::new(&mut clipped);

                    to_embedded_rect(rect)
                        .into_styled(style)
                        .draw(&mut dithered)
                }
            }
        };

        self.record(result);
    }

    fn line(&mut self, start: Point, end: Point, width: Pixels, color: Color) {
        if self.error.is_some() || width.is_non_positive() {
            return;
        }

        let start = self.translated_point(start);
        let end = self.translated_point(end);

        let width = u32::try_from(width.non_negative().get()).unwrap_or(u32::MAX);

        let clip = to_embedded_rect(self.clip);

        let style = EgPrimitiveStyle::with_stroke(color_to_gray2(color), width);

        let result = {
            let mut clipped = self.target.clipped(&clip);

            match self.ui_mode {
                EInkUiMode::NativeGray2 => {
                    EgLine::new(to_embedded_point(start), to_embedded_point(end))
                        .into_styled(style)
                        .draw(&mut clipped)
                }
                EInkUiMode::BinaryDither => {
                    let mut dithered = BinaryDitherTarget::new(&mut clipped);

                    EgLine::new(to_embedded_point(start), to_embedded_point(end))
                        .into_styled(style)
                        .draw(&mut dithered)
                }
            }
        };

        self.record(result);
    }

    fn fill_circle(&mut self, center: Point, radius: Pixels, color: Color) {
        if self.error.is_some() || radius.is_non_positive() {
            return;
        }

        let center = self.translated_point(center);

        let diameter = radius.non_negative().get().saturating_mul(2);
        let diameter = u32::try_from(diameter).unwrap_or(u32::MAX);

        let clip = to_embedded_rect(self.clip);

        let style = EgPrimitiveStyle::with_fill(color_to_gray2(color));

        let result = {
            let mut clipped = self.target.clipped(&clip);

            match self.ui_mode {
                EInkUiMode::NativeGray2 => {
                    EgCircle::with_center(to_embedded_point(center), diameter)
                        .into_styled(style)
                        .draw(&mut clipped)
                }
                EInkUiMode::BinaryDither => {
                    let mut dithered = BinaryDitherTarget::new(&mut clipped);

                    EgCircle::with_center(to_embedded_point(center), diameter)
                        .into_styled(style)
                        .draw(&mut dithered)
                }
            }
        };

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

        let clip = to_embedded_rect(self.clip);

        let style = EgPrimitiveStyleBuilder::new()
            .stroke_color(color_to_gray2(color))
            .stroke_width(width)
            .stroke_alignment(EgStrokeAlignment::Inside)
            .build();

        let result = {
            let mut clipped = self.target.clipped(&clip);

            match self.ui_mode {
                EInkUiMode::NativeGray2 => {
                    EgCircle::with_center(to_embedded_point(center), diameter)
                        .into_styled(style)
                        .draw(&mut clipped)
                }
                EInkUiMode::BinaryDither => {
                    let mut dithered = BinaryDitherTarget::new(&mut clipped);

                    EgCircle::with_center(to_embedded_point(center), diameter)
                        .into_styled(style)
                        .draw(&mut dithered)
                }
            }
        };

        self.record(result);
    }

    fn fill_pixel_coverage(&mut self, point: Point, color: Color, coverage: u8) {
        if self.error.is_some() || coverage == 0 {
            return;
        }

        let point = self.translated_point(point);

        if !self.clip.contains(point) {
            return;
        }

        let point = to_embedded_point(point);

        let output = match self.coverage_mode {
            EInkCoverageMode::BinaryThreshold => {
                if coverage < 128 {
                    return;
                }

                color_to_gray2(color)
            }
            EInkCoverageMode::OrderedDither4x4 => {
                if !ordered_dither_accepts(coverage, point) {
                    return;
                }

                binary_dither_gray2(color_to_gray2(color), point)
            }
            EInkCoverageMode::AlphaBlend { read_pixel } => {
                if coverage == u8::MAX {
                    color_to_gray2(color)
                } else {
                    let Some(background) = read_pixel(&*self.target, point) else {
                        return;
                    };

                    alpha_blend_gray2(color, background, coverage)
                }
            }
        };

        let result = self
            .target
            .draw_iter(core::iter::once(EgPixel(point, output)));

        self.record(result);
    }
}
