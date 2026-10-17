use embedded_graphics::{
    Drawable, Pixel as EgPixel,
    draw_target::DrawTargetExt,
    geometry::Point as EgPoint,
    pixelcolor::GrayColor,
    prelude::DrawTarget as EgDrawTarget,
    primitives::{Primitive, PrimitiveStyle as EgPrimitiveStyle},
};

use crate::{
    BoxPaint, CanvasPainter, Color, DamageRegion, GlyphBitmap, Painter, Point, Rect,
    backend::{
        eink::tone::{BinaryDitherTarget, gray2_tone},
        embedded::{draw_box_to, from_embedded_size, to_embedded_point, to_embedded_rect},
    },
    px,
};

mod canvas;
mod coverage;
mod tone;

pub use coverage::{
    DEFAULT_MIN_INK_COVERAGE, EInkCoverageBitmap, EInkCoverageBlitter, EInkCoverageMode, inks,
};
pub use tone::{EInkPaintReport, EInkTone, EInkUiMode};

pub use embedded_graphics::pixelcolor::Gray2;

use canvas::EInkCanvasPainter;
use coverage::{color_to_gray2, draw_coverage_bitmap};

pub struct EInkPainter<'target, D>
where
    D: EgDrawTarget<Color = Gray2>,
{
    target: &'target mut D,
    coverage_mode: EInkCoverageMode<D>,
    coverage_blitter: Option<EInkCoverageBlitter<D>>,
    min_ink_coverage: u8,
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
            coverage_blitter: None,
            min_ink_coverage: DEFAULT_MIN_INK_COVERAGE,
            ui_mode: EInkUiMode::NativeGray2,
            report: EInkPaintReport::default(),
        }
    }

    pub fn with_coverage_mode(mut self, coverage_mode: EInkCoverageMode<D>) -> Self {
        self.coverage_mode = coverage_mode;
        self
    }

    pub fn with_coverage_blitter(mut self, blitter: EInkCoverageBlitter<D>) -> Self {
        self.coverage_blitter = Some(blitter);
        self
    }

    /// the least glyph or path coverage that inks a pixel in `BinaryDither` mode
    pub fn with_min_ink_coverage(mut self, min_coverage: u8) -> Self {
        self.min_ink_coverage = min_coverage;
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

    fn effective_coverage_mode(&self) -> EInkCoverageMode<D> {
        match self.ui_mode {
            EInkUiMode::NativeGray2 => self.coverage_mode,
            EInkUiMode::BinaryDither => match self.coverage_blitter {
                Some(blitter) => {
                    EInkCoverageMode::ink_threshold_with_blitter(self.min_ink_coverage, blitter)
                }
                None => EInkCoverageMode::ink_threshold(self.min_ink_coverage),
            },
        }
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
    type Error = D::Error;

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

        match self.ui_mode {
            EInkUiMode::NativeGray2 => {
                if let Some(clip) = clip {
                    let clip = to_embedded_rect(clip);
                    let mut clipped = self.target.clipped(&clip);

                    draw_box_to(&mut clipped, bounds, paint, color_to_gray2)
                } else {
                    draw_box_to(self.target, bounds, paint, color_to_gray2)
                }
            }

            EInkUiMode::BinaryDither => {
                if let Some(clip) = clip {
                    let clip = to_embedded_rect(clip);
                    let mut clipped = self.target.clipped(&clip);
                    let mut dithered = BinaryDitherTarget::new(&mut clipped);

                    draw_box_to(&mut dithered, bounds, paint, color_to_gray2)
                } else {
                    let mut dithered = BinaryDitherTarget::new(self.target);

                    draw_box_to(&mut dithered, bounds, paint, color_to_gray2)
                }
            }
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

        let coverage_mode = self.effective_coverage_mode();

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

        painter.finish()
    }

    fn draw_glyph(
        &mut self,
        bitmap: &GlyphBitmap<'_>,
        origin: Point,
        color: Color,
        clip: Rect,
    ) -> Result<(), Self::Error> {
        if self.ui_mode == EInkUiMode::NativeGray2 {
            self.record_native_ui_color(color);

            if matches!(self.coverage_mode, EInkCoverageMode::AlphaBlend { .. }) {
                // partial glyph coverage can create intermediate Gray2 output even
                // for a binary foreground color.
                self.report.include(EInkTone::Gray4);
            }
        }

        let coverage_mode = self.effective_coverage_mode();

        draw_coverage_bitmap(
            self.target,
            &mut self.report,
            bitmap,
            origin,
            color,
            clip,
            coverage_mode,
        )
    }

    fn fill_rect(&mut self, rect: Rect, color: Color, clip: Rect) -> Result<(), Self::Error> {
        self.record_native_ui_color(color);

        to_embedded_rect(rect)
            .into_styled(EgPrimitiveStyle::with_fill(color_to_gray2(color)))
            .draw(&mut self.target.clipped(&to_embedded_rect(clip)))
    }

    fn draw_pixels(&mut self, origin: Point, pixels: &[Color]) -> Result<(), Self::Error> {
        let report = &mut self.report;
        let y = origin.y.get();

        let pixels = pixels.iter().zip(origin.x.get()..).map(|(&color, x)| {
            let gray = color_to_gray2(color);

            if matches!(gray.luma(), 1 | 2) {
                report.include(EInkTone::Gray4);
            }

            EgPixel(EgPoint::new(x, y), gray)
        });

        self.target.draw_iter(pixels)
    }
}
