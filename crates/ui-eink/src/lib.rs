//! Paints the ui onto 2-bit grayscale e-paper framebuffers through
//! embedded-graphics, tracking which grey levels a frame needs.

#![no_std]

#[cfg(test)]
extern crate std;

use embedded_graphics::{
    Drawable, Pixel as EgPixel,
    draw_target::DrawTargetExt,
    geometry::Point as EgPoint,
    pixelcolor::GrayColor,
    prelude::DrawTarget as EgDrawTarget,
    primitives::{Primitive, PrimitiveStyle as EgPrimitiveStyle},
};

use inkpaper_ui::{
    BoxPaint, CanvasPainter, Color, DamageRegion, GlyphBitmap, Painter, Point, Rect,
    backend::{draw_box_to, from_embedded_size, to_embedded_rect},
    px,
};

use tone::{BinaryDitherTarget, gray2_tone};

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
    read_pixel: Option<fn(&D, EgPoint) -> Option<Gray2>>,
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
            read_pixel: None,
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

    /// reads back the target's pixels, so antialiased glyphs blend into what's
    /// under them in `BinaryDither` mode. Without it, they're black and white
    /// like other text
    pub fn with_pixel_reader(mut self, read_pixel: fn(&D, EgPoint) -> Option<Gray2>) -> Self {
        self.read_pixel = Some(read_pixel);
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

        let coverage_mode = match self.read_pixel {
            Some(read_pixel) if bitmap.antialias() && self.ui_mode == EInkUiMode::BinaryDither => {
                EInkCoverageMode::alpha_blend(read_pixel)
            }
            _ => self.effective_coverage_mode(),
        };

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

#[cfg(test)]
mod tests {
    use embedded_graphics::{mock_display::MockDisplay, pixelcolor::GrayColor};
    use inkpaper_ui::{GlyphMetrics, Size, TextDarkness};

    use super::*;

    /// a black glyph a row of four pixels wide, from empty to fully covered
    const COVERAGE: [u8; 4] = [0, 64, 160, 255];

    fn white_display() -> MockDisplay<Gray2> {
        let mut display = MockDisplay::new();
        display.set_allow_overdraw(true);
        display.clear(Gray2::WHITE).unwrap();
        display
    }

    fn read_pixel(display: &MockDisplay<Gray2>, point: EgPoint) -> Option<Gray2> {
        display.get_pixel(point)
    }

    /// the row's levels after drawing the glyph, and the frame's tone
    fn draw(antialias: bool, reader: bool, darkness: TextDarkness) -> ([u8; 4], EInkTone) {
        draw_row(&COVERAGE, antialias, reader, darkness)
    }

    fn draw_row(
        coverage: &[u8; 4],
        antialias: bool,
        reader: bool,
        darkness: TextDarkness,
    ) -> ([u8; 4], EInkTone) {
        let mut display = white_display();

        let metrics = GlyphMetrics {
            width: 4,
            height: 1,
            ..GlyphMetrics::default()
        };
        let bitmap = GlyphBitmap::new(coverage, metrics)
            .with_antialias(antialias)
            .with_darkness(darkness);
        let clip = Rect::new(Point::ZERO, Size::new(px(4), px(1)));

        let tone = {
            let painter = EInkPainter::new(&mut display).with_ui_mode(EInkUiMode::BinaryDither);
            let mut painter = if reader {
                painter.with_pixel_reader(read_pixel)
            } else {
                painter
            };

            painter
                .draw_glyph(&bitmap, Point::ZERO, Color::BLACK, clip)
                .unwrap();

            painter.report().tone()
        };

        let levels =
            core::array::from_fn(|x| display.get_pixel(EgPoint::new(x as i32, 0)).unwrap().luma());

        (levels, tone)
    }

    #[test]
    fn antialiased_glyphs_blend_into_native_gray() {
        assert_eq!(
            draw(true, true, TextDarkness::NORMAL),
            ([3, 2, 1, 0], EInkTone::Gray4)
        );
    }

    #[test]
    fn darkness_moves_antialiased_edges_but_keeps_stems_black() {
        let light = TextDarkness::percent(75);
        let extra_dark = TextDarkness::percent(180);

        // light pales a dark edge, and the stem stays black
        assert_eq!(draw(true, true, light).0, [3, 2, 2, 0]);

        // extra dark inks a faint edge normal leaves white
        let faint = [0, 40, 160, 255];
        assert_eq!(
            draw_row(&faint, true, true, TextDarkness::NORMAL).0,
            [3, 3, 1, 0]
        );
        assert_eq!(draw_row(&faint, true, true, extra_dark).0, [3, 2, 1, 0]);
    }

    #[test]
    fn glyphs_stay_binary_without_antialiasing_or_a_reader() {
        let binary = ([3, 3, 0, 0], EInkTone::Binary);

        assert_eq!(draw(false, true, TextDarkness::NORMAL), binary);
        assert_eq!(draw(true, false, TextDarkness::NORMAL), binary);
    }
}
