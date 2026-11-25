use skrifa::{
    FontRef, GlyphId as SkrifaGlyphId, MetadataProvider,
    instance::{LocationRef, Size},
};

use crate::{
    FontMetrics, GlyphMetrics, Pixels,
    float::{ceil_i32, floor_i32, round_i32},
    px,
};

pub(super) fn positive_scaled(value: f32) -> Pixels {
    if value <= 0.0 || !value.is_finite() {
        return px(0);
    }

    px(ceil_i32(value))
}

pub(super) fn font_metrics_for_font(
    font: &FontRef<'_>,
    location: LocationRef<'_>,
    size_px: u16,
) -> FontMetrics {
    if size_px == 0 {
        return FontMetrics::default();
    }

    let metrics = font.metrics(Size::new(f32::from(size_px)), location);
    if metrics.units_per_em == 0 {
        return FontMetrics::default();
    }

    FontMetrics::new(
        positive_scaled(metrics.ascent),
        positive_scaled(-metrics.descent),
        positive_scaled(metrics.leading),
    )
}

pub(super) fn glyph_advance_for_font(
    font: &FontRef<'_>,
    location: LocationRef<'_>,
    glyph: SkrifaGlyphId,
    size_px: u16,
) -> Option<Pixels> {
    if size_px == 0 {
        return None;
    }

    let metrics = font.glyph_metrics(Size::new(f32::from(size_px)), location);
    let advance = metrics.advance_width(glyph)?;

    Some(px(round_i32(advance)))
}

pub(super) fn glyph_metrics_for_font(
    font: &FontRef<'_>,
    location: LocationRef<'_>,
    glyph: SkrifaGlyphId,
    size_px: u16,
) -> Option<GlyphMetrics> {
    if size_px == 0 {
        return None;
    }

    let metrics = font.glyph_metrics(Size::new(f32::from(size_px)), location);
    let advance = px(round_i32(metrics.advance_width(glyph)?));

    let Some(bounds) = metrics.bounds(glyph) else {
        return Some(GlyphMetrics::new(0, 0, px(0), px(0), advance));
    };

    // font coordinates are Y-up. Our framebuffer coordinates are Y-down
    let left = floor_i32(bounds.x_min);
    let right = ceil_i32(bounds.x_max);
    let top = floor_i32(-bounds.y_max);
    let bottom = ceil_i32(-bounds.y_min);
    let width = right.saturating_sub(left).max(0);
    let height = bottom.saturating_sub(top).max(0);

    if width == 0 || height == 0 {
        return Some(GlyphMetrics::new(0, 0, px(0), px(0), advance));
    }

    Some(GlyphMetrics::new(
        u16::try_from(width).ok()?,
        u16::try_from(height).ok()?,
        px(left),
        px(top),
        advance,
    ))
}
