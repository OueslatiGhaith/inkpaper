use embedded_graphics::{
    Pixel as EgPixel,
    geometry::Point as EgPoint,
    pixelcolor::{Gray2, GrayColor},
    prelude::DrawTarget as EgDrawTarget,
};

use crate::{
    Color, GlyphBitmap, Luminance, Point, Rect,
    backend::{EInkPaintReport, eink::tone::binary_dither_gray2},
    increment_metric,
};

use super::EInkError;

#[derive(Debug, Clone, Copy)]
pub struct EInkOrderedCoverageBitmap<'a> {
    coverage: &'a [u8],
    width: u16,
    height: u16,
    origin: Point,
    foreground: Gray2,
    clip: Rect,
}

impl<'a> EInkOrderedCoverageBitmap<'a> {
    pub fn new(
        coverage: &'a [u8],
        width: u16,
        height: u16,
        origin: Point,
        foreground: Gray2,
        clip: Rect,
    ) -> Self {
        debug_assert_eq!(
            coverage.len(),
            usize::from(width).saturating_mul(usize::from(height))
        );

        Self {
            coverage,
            width,
            height,
            origin,
            foreground,
            clip,
        }
    }

    pub const fn coverage(self) -> &'a [u8] {
        self.coverage
    }

    pub const fn width(self) -> u16 {
        self.width
    }

    pub const fn height(self) -> u16 {
        self.height
    }

    pub const fn origin(self) -> Point {
        self.origin
    }

    pub const fn foreground(self) -> Gray2 {
        self.foreground
    }

    pub const fn clip(self) -> Rect {
        self.clip
    }
}

pub type EInkOrderedCoverageBlitter<D> =
    for<'bitmap> fn(&mut D, EInkOrderedCoverageBitmap<'bitmap>) -> Option<u64>;

#[derive(Debug)]
pub enum EInkCoverageMode<D> {
    BinaryThreshold,
    OrderedDither4x4 {
        blitter: Option<EInkOrderedCoverageBlitter<D>>,
    },
    AlphaBlend {
        read_pixel: fn(&D, EgPoint) -> Option<Gray2>,
    },
}

impl<D> Copy for EInkCoverageMode<D> {}

impl<D> Clone for EInkCoverageMode<D> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<D> EInkCoverageMode<D> {
    pub const fn binary_threshold() -> Self {
        Self::BinaryThreshold
    }

    pub const fn ordered_dither_4x4() -> Self {
        Self::OrderedDither4x4 { blitter: None }
    }

    pub const fn ordered_dither_4x4_with_blitter(blitter: EInkOrderedCoverageBlitter<D>) -> Self {
        Self::OrderedDither4x4 {
            blitter: Some(blitter),
        }
    }

    pub const fn alpha_blend(read_pixel: fn(&D, EgPoint) -> Option<Gray2>) -> Self {
        Self::AlphaBlend { read_pixel }
    }
}

pub(super) fn color_to_gray2(color: Color) -> Gray2 {
    luminance_to_gray2(color.luminance())
}

pub(super) fn luminance_to_gray2(luminance: Luminance) -> Gray2 {
    let value = u32::from(luminance.get());

    let level = (value.saturating_mul(3).saturating_add(127)) / 255;

    Gray2::new(level as u8)
}

fn gray2_luminance(color: Gray2) -> Luminance {
    Luminance::new(color.luma().saturating_mul(85))
}

pub(super) fn alpha_blend_gray2(foreground: Color, background: Gray2, coverage: u8) -> Gray2 {
    if coverage == 0 {
        return background;
    }

    if coverage == u8::MAX {
        return color_to_gray2(foreground);
    }

    let foreground = u32::from(foreground.luminance().get());
    let background = u32::from(gray2_luminance(background).get());

    let alpha = u32::from(coverage);
    let inverse = 255u32.saturating_sub(alpha);

    let blended = foreground
        .saturating_mul(alpha)
        .saturating_add(background.saturating_mul(inverse))
        .saturating_add(127)
        / 255;

    luminance_to_gray2(Luminance::new(u8::try_from(blended).unwrap_or(u8::MAX)))
}

pub(super) fn draw_coverage_bitmap<D>(
    target: &mut D,
    report: &mut EInkPaintReport,
    bitmap: &GlyphBitmap<'_>,
    origin: Point,
    color: Color,
    clip: Rect,
    coverage_mode: EInkCoverageMode<D>,
) -> Result<(), EInkError<D::Error>>
where
    D: EgDrawTarget<Color = Gray2>,
{
    let width = usize::from(bitmap.width());
    let height = usize::from(bitmap.height());

    if width == 0 || height == 0 {
        #[cfg(feature = "metrics")]
        report.record_coverage_bitmap(0, 0);

        #[cfg(not(feature = "metrics"))]
        let _ = report;

        return Ok(());
    }

    let coverage = bitmap.coverage();

    debug_assert_eq!(coverage.len(), width.saturating_mul(height));

    let foreground = color_to_gray2(color);

    #[cfg(feature = "metrics")]
    let samples = u64::try_from(coverage.len()).unwrap_or(u64::MAX);

    match coverage_mode {
        EInkCoverageMode::BinaryThreshold => {
            #[cfg(feature = "metrics")]
            let mut accepted = 0u64;

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

                    increment_metric!(accepted);

                    Some(EgPixel(point, foreground))
                });

            let result = target.draw_iter(pixels).map_err(EInkError::Target);

            #[cfg(feature = "metrics")]
            report.record_coverage_bitmap(samples, accepted);

            #[cfg(not(feature = "metrics"))]
            let _ = report;

            result
        }

        EInkCoverageMode::OrderedDither4x4 { blitter } => {
            if let Some(blitter) = blitter {
                let bitmap = EInkOrderedCoverageBitmap::new(
                    coverage,
                    bitmap.width(),
                    bitmap.height(),
                    origin,
                    foreground,
                    clip,
                );

                if let Some(accepted) = blitter(target, bitmap) {
                    #[cfg(feature = "metrics")]
                    report.record_coverage_bitmap(samples, accepted);

                    #[cfg(not(feature = "metrics"))]
                    {
                        let _ = report;
                        let _ = accepted;
                    }

                    return Ok(());
                }
            }

            #[cfg(feature = "metrics")]
            let mut accepted = 0u64;

            let pixels = coverage
                .iter()
                .copied()
                .enumerate()
                .filter_map(|(index, coverage)| {
                    let point = coverage_point(origin, width, index)?;

                    if !point_in_rect(point, clip) || !ordered_dither_accepts(coverage, point) {
                        return None;
                    }

                    increment_metric!(accepted);

                    Some(EgPixel(point, binary_dither_gray2(foreground, point)))
                });

            let result = target.draw_iter(pixels).map_err(EInkError::Target);

            #[cfg(feature = "metrics")]
            report.record_coverage_bitmap(samples, accepted);

            #[cfg(not(feature = "metrics"))]
            let _ = report;

            result
        }

        EInkCoverageMode::AlphaBlend { read_pixel } => {
            #[cfg(feature = "metrics")]
            let mut accepted = 0u64;

            let solid_pixels =
                coverage
                    .iter()
                    .copied()
                    .enumerate()
                    .filter_map(|(index, coverage)| {
                        if coverage != u8::MAX {
                            return None;
                        }

                        let point = coverage_point(origin, width, index)?;

                        if !point_in_rect(point, clip) {
                            return None;
                        }

                        increment_metric!(accepted);

                        Some(EgPixel(point, foreground))
                    });

            target.draw_iter(solid_pixels).map_err(EInkError::Target)?;

            for (index, coverage) in coverage.iter().copied().enumerate() {
                if coverage == 0 || coverage == u8::MAX {
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

                let color = alpha_blend_gray2(color, background, coverage);

                target
                    .draw_iter(core::iter::once(EgPixel(point, color)))
                    .map_err(EInkError::Target)?;

                increment_metric!(accepted);
            }

            #[cfg(feature = "metrics")]
            report.record_coverage_bitmap(samples, accepted);

            #[cfg(not(feature = "metrics"))]
            let _ = report;

            Ok(())
        }
    }
}

fn coverage_point(origin: Point, width: usize, index: usize) -> Option<EgPoint> {
    if width == 0 {
        return None;
    }

    let x = i32::try_from(index % width).ok()?;
    let y = i32::try_from(index / width).ok()?;

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

pub(super) fn ordered_dither_accepts(coverage: u8, point: EgPoint) -> bool {
    if coverage == 0 {
        return false;
    }

    if coverage == u8::MAX {
        return true;
    }

    const BAYER_4X4: [u8; 16] = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5];

    let x = point.x.rem_euclid(4) as usize;
    let y = point.y.rem_euclid(4) as usize;

    let rank = BAYER_4X4[y * 4 + x];
    let threshold = rank.saturating_mul(16).saturating_add(8);

    coverage > threshold
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_colors_quantize_to_four_native_levels() {
        assert_eq!(color_to_gray2(Color::BLACK).luma(), 0);
        assert_eq!(color_to_gray2(Color::RED).luma(), 1);
        assert_eq!(color_to_gray2(Color::rgb(170, 170, 170)).luma(), 2);
        assert_eq!(color_to_gray2(Color::WHITE).luma(), 3);
    }

    #[test]
    fn gray2_levels_expand_to_even_luminance_steps() {
        assert_eq!(gray2_luminance(Gray2::new(0)), Luminance::new(0));
        assert_eq!(gray2_luminance(Gray2::new(1)), Luminance::new(85));
        assert_eq!(gray2_luminance(Gray2::new(2)), Luminance::new(170));
        assert_eq!(gray2_luminance(Gray2::new(3)), Luminance::new(255));
    }

    #[test]
    fn full_coverage_does_not_read_or_blend_background() {
        assert_eq!(
            alpha_blend_gray2(Color::BLACK, Gray2::new(3), u8::MAX),
            Gray2::new(0)
        );
    }
}
