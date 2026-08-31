use embedded_graphics::{
    Pixel as EgPixel,
    geometry::Point as EgPoint,
    pixelcolor::{Rgb888 as EgRgb888, RgbColor},
    prelude::DrawTarget as EgDrawTarget,
};

use crate::{GlyphBitmap, Point, Rect};

use super::EmbeddedGraphicsError;

#[derive(Debug)]
pub enum CoverageMode<D> {
    BinaryThreshold,
    OrderedDither4x4,
    AlphaBlend {
        read_pixel: fn(&D, EgPoint) -> Option<EgRgb888>,
    },
}

impl<D> Copy for CoverageMode<D> {}
impl<D> Clone for CoverageMode<D> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<D> CoverageMode<D> {
    pub const fn binary_threshold() -> Self {
        Self::BinaryThreshold
    }

    pub const fn ordered_dither_4x4() -> Self {
        Self::OrderedDither4x4
    }

    pub const fn alpha_blend(read_pixel: fn(&D, EgPoint) -> Option<EgRgb888>) -> Self {
        Self::AlphaBlend { read_pixel }
    }
}

pub(super) fn draw_coverage_bitmap<D>(
    target: &mut D,
    bitmap: &GlyphBitmap<'_>,
    origin: Point,
    color: EgRgb888,
    clip: Rect,
    coverage_mode: CoverageMode<D>,
) -> Result<(), EmbeddedGraphicsError<D::Error>>
where
    D: EgDrawTarget,
    D::Color: From<EgRgb888>,
{
    let width = usize::from(bitmap.width());
    let height = usize::from(bitmap.height());

    if width == 0 || height == 0 {
        return Ok(());
    }

    let coverage = bitmap.coverage();
    debug_assert_eq!(coverage.len(), width.saturating_mul(height,),);

    match coverage_mode {
        CoverageMode::BinaryThreshold => {
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

                    Some(EgPixel(point, D::Color::from(color)))
                });

            target
                .draw_iter(pixels)
                .map_err(EmbeddedGraphicsError::Target)
        }
        CoverageMode::OrderedDither4x4 => {
            let pixels = coverage
                .iter()
                .copied()
                .enumerate()
                .filter_map(|(index, coverage)| {
                    let point = coverage_point(origin, width, index)?;
                    if !point_in_rect(point, clip) {
                        return None;
                    }

                    if !ordered_dither_accepts(coverage, point) {
                        return None;
                    }

                    Some(EgPixel(point, D::Color::from(color)))
                });

            target
                .draw_iter(pixels)
                .map_err(EmbeddedGraphicsError::Target)
        }
        CoverageMode::AlphaBlend { read_pixel } => {
            // fully covered pixels need no destination readback. Keep those together in
            // one DrawTarget call. Only edge pixels require the slower read/blend/write path
            let solid_pixels =
                coverage
                    .iter()
                    .copied()
                    .enumerate()
                    .filter_map(|(index, coverage)| {
                        if coverage != 255 {
                            return None;
                        }

                        let point = coverage_point(origin, width, index)?;
                        if !point_in_rect(point, clip) {
                            return None;
                        }

                        Some(EgPixel(point, D::Color::from(color)))
                    });

            target
                .draw_iter(solid_pixels)
                .map_err(EmbeddedGraphicsError::Target)?;

            for (index, coverage) in coverage.iter().copied().enumerate() {
                if coverage == 0 || coverage == 255 {
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

                let blended = alpha_blend_rgb888(color, background, coverage);

                target
                    .draw_iter(core::iter::once(EgPixel(point, D::Color::from(blended))))
                    .map_err(EmbeddedGraphicsError::Target)?;
            }

            Ok(())
        }
    }
}

fn coverage_point(origin: Point, width: usize, index: usize) -> Option<EgPoint> {
    if width == 0 {
        return None;
    }

    let x = index % width;
    let x = i32::try_from(x).ok()?;
    let y = index / width;
    let y = i32::try_from(y).ok()?;

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

pub(crate) fn alpha_blend_rgb888(
    foreground: EgRgb888,
    background: EgRgb888,
    coverage: u8,
) -> EgRgb888 {
    EgRgb888::new(
        alpha_blend_channel(foreground.r(), background.r(), coverage),
        alpha_blend_channel(foreground.g(), background.g(), coverage),
        alpha_blend_channel(foreground.b(), background.b(), coverage),
    )
}

fn alpha_blend_channel(foreground: u8, background: u8, coverage: u8) -> u8 {
    let alpha = u32::from(coverage);
    let inverse = 255u32.saturating_sub(alpha);
    let value = u32::from(foreground)
        .saturating_mul(alpha)
        .saturating_add(u32::from(background).saturating_mul(inverse))
        .saturating_add(127)
        / 255;

    u8::try_from(value).unwrap_or(u8::MAX)
}

const BAYER_4X4: [u8; 16] = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5];

pub(crate) fn ordered_dither_accepts(coverage: u8, point: EgPoint) -> bool {
    if coverage == 0 {
        return false;
    }

    if coverage == 255 {
        return true;
    }

    let x = point.x.rem_euclid(4) as usize;
    let y = point.y.rem_euclid(4) as usize;
    let rank = BAYER_4X4[y * 4 + x];
    let threshold = rank.saturating_mul(16).saturating_add(8);

    coverage > threshold
}
