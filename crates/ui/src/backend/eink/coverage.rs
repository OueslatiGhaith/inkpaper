use embedded_graphics::{
    Pixel as EgPixel,
    geometry::Point as EgPoint,
    pixelcolor::{Gray2, GrayColor},
    prelude::DrawTarget as EgDrawTarget,
};

use crate::{Color, GlyphBitmap, Luminance, Point, Rect, backend::eink::tone::binary_dither_gray2};

use super::EInkError;

#[derive(Debug)]
pub enum EInkCoverageMode<D> {
    BinaryThreshold,
    OrderedDither4x4,
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
        Self::OrderedDither4x4
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
        return Ok(());
    }

    let coverage = bitmap.coverage();

    debug_assert_eq!(coverage.len(), width.saturating_mul(height));

    let foreground = color_to_gray2(color);

    match coverage_mode {
        EInkCoverageMode::BinaryThreshold => {
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

                    Some(EgPixel(point, foreground))
                });

            target.draw_iter(pixels).map_err(EInkError::Target)
        }

        EInkCoverageMode::OrderedDither4x4 => {
            let pixels = coverage
                .iter()
                .copied()
                .enumerate()
                .filter_map(|(index, coverage)| {
                    let point = coverage_point(origin, width, index)?;

                    if !point_in_rect(point, clip) || !ordered_dither_accepts(coverage, point) {
                        return None;
                    }

                    Some(EgPixel(point, binary_dither_gray2(foreground, point)))
                });

            target.draw_iter(pixels).map_err(EInkError::Target)
        }

        EInkCoverageMode::AlphaBlend { read_pixel } => {
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
            }

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
