use ttf_parser::{Face, GlyphId as TtfGlyphId};

use crate::{GlyphMetrics, Pixels, px};

pub(super) fn font_scale(face: &Face<'_>, size_px: u16) -> Option<f32> {
    if size_px == 0 {
        return None;
    }

    let units_per_em = face.units_per_em();
    if units_per_em == 0 {
        return None;
    }

    Some(f32::from(size_px) / f32::from(units_per_em))
}

pub(super) fn positive_scaled_units(units: i32, scale: f32) -> Pixels {
    if units <= 0 {
        return px(0);
    }

    px(libm::ceilf(units as f32 * scale) as i32)
}

pub(super) fn glyph_advance_for_face(
    face: &Face<'_>,
    glyph: TtfGlyphId,
    size_px: u16,
) -> Option<Pixels> {
    let scale = font_scale(face, size_px)?;
    let advance = face.glyph_hor_advance(glyph)?;

    Some(px(libm::roundf(f32::from(advance) * scale) as i32))
}

pub(super) fn glyph_metrics_for_face(
    face: &Face<'_>,
    glyph: TtfGlyphId,
    size_px: u16,
) -> Option<GlyphMetrics> {
    let scale = font_scale(face, size_px)?;
    let advance = glyph_advance_for_face(face, glyph, size_px)?;

    let Some(bounds) = face.glyph_bounding_box(glyph) else {
        return Some(GlyphMetrics::new(0, 0, px(0), px(0), advance));
    };

    // font coordinates are Y-up. Our framebuffer coordinates are Y-down
    let left = libm::floorf(f32::from(bounds.x_min) * scale) as i32;
    let right = libm::ceilf(f32::from(bounds.x_max) * scale) as i32;
    let top = libm::floorf(-f32::from(bounds.y_max) * scale) as i32;
    let bottom = libm::ceilf(-f32::from(bounds.y_min) * scale) as i32;
    let width = right.saturating_sub(left).max(0);
    let height = bottom.saturating_sub(top).max(0);

    Some(GlyphMetrics::new(
        u16::try_from(width).ok()?,
        u16::try_from(height).ok()?,
        px(left),
        px(top),
        advance,
    ))
}
