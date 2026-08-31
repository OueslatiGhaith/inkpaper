use ttf_parser::{Face, GlyphId as TtfGlyphId};

use crate::{
    CursiveAttachment, FontData, FontFace, FontMetrics, FontRasterError, GlyphId, GlyphMetrics,
    Offset, OpenTypeFeature, Pixels, px,
};

mod gpos;
mod gsub;
mod metrics;
mod raster;

use gpos::{
    gpos_cursive_attachment_for_face, gpos_kerning_for_face, legacy_kerning_for_face,
    mark_to_base_offset_for_face, mark_to_ligature_offset_for_face, mark_to_mark_offset_for_face,
};
use gsub::{gsub_pair_ligature_for_face, gsub_single_substitution_for_face};
use metrics::{font_scale, glyph_advance_for_face, glyph_metrics_for_face, positive_scaled_units};
use raster::{SUPERSAMPLE_Y, ScanlineBuilder, accumulate_scanline, normalize_coverage};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum TtfFontError {
    InvalidFont,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct TtfFont<'a> {
    data: FontData<'a>,
    face_index: u32,
}

impl<'a> TtfFont<'a> {
    pub fn parse(data: FontData<'a>, face_index: u32) -> Result<Self, TtfFontError> {
        Face::parse(data.bytes(), face_index).map_err(|_| TtfFontError::InvalidFont)?;

        Ok(Self { data, face_index })
    }

    pub const fn data(self) -> FontData<'a> {
        self.data
    }

    pub const fn face_index(self) -> u32 {
        self.face_index
    }

    fn face(&self) -> Result<Face<'a>, TtfFontError> {
        Face::parse(self.data.bytes(), self.face_index).map_err(|_| TtfFontError::InvalidFont)
    }
}

impl FontFace for TtfFont<'_> {
    fn glyph_id(&self, character: char) -> Option<GlyphId> {
        let face = self.face().ok()?;
        let glyph = face.glyph_index(character)?;

        Some(GlyphId::new(glyph.0))
    }

    fn metrics(&self, size_px: u16) -> FontMetrics {
        let Ok(face) = self.face() else {
            return FontMetrics::default();
        };

        let Some(scale) = font_scale(&face, size_px) else {
            return FontMetrics::default();
        };

        FontMetrics::new(
            positive_scaled_units(i32::from(face.ascender()), scale),
            positive_scaled_units(i32::from(face.descender()).saturating_neg(), scale),
            positive_scaled_units(i32::from(face.line_gap()), scale),
        )
    }

    fn glyph_advance(&self, glyph: GlyphId, size_px: u16) -> Option<Pixels> {
        let face = self.face().ok()?;

        glyph_advance_for_face(&face, to_ttf_glyph(glyph), size_px)
    }

    fn glyph_metrics(&self, glyph: GlyphId, size_px: u16) -> Option<GlyphMetrics> {
        let face = self.face().ok()?;

        glyph_metrics_for_face(&face, to_ttf_glyph(glyph), size_px)
    }

    fn kerning(&self, left: GlyphId, right: GlyphId, size_px: u16) -> Pixels {
        let Ok(face) = self.face() else { return px(0) };

        let left = to_ttf_glyph(left);
        let right = to_ttf_glyph(right);

        if let Some(adjustment) = gpos_kerning_for_face(&face, left, right, size_px) {
            return adjustment;
        }

        legacy_kerning_for_face(&face, left, right, size_px).unwrap_or(px(0))
    }

    fn single_substitution(&self, feature: OpenTypeFeature, glyph: GlyphId) -> Option<GlyphId> {
        let face = self.face().ok()?;

        let substituted = gsub_single_substitution_for_face(&face, feature, to_ttf_glyph(glyph))?;

        Some(GlyphId::new(substituted.0))
    }

    fn ligature_substitution(
        &self,
        feature: OpenTypeFeature,
        first: GlyphId,
        second: GlyphId,
    ) -> Option<GlyphId> {
        let face = self.face().ok()?;

        let substituted =
            gsub_pair_ligature_for_face(&face, feature, to_ttf_glyph(first), to_ttf_glyph(second))?;

        Some(GlyphId::new(substituted.0))
    }

    fn cursive_attachment(
        &self,
        visual_left: GlyphId,
        visual_right: GlyphId,
        size_px: u16,
        right_to_left: bool,
    ) -> Option<CursiveAttachment> {
        let face = self.face().ok()?;

        gpos_cursive_attachment_for_face(
            &face,
            to_ttf_glyph(visual_left),
            to_ttf_glyph(visual_right),
            size_px,
            right_to_left,
        )
    }

    fn mark_to_base_offset(&self, base: GlyphId, mark: GlyphId, size_px: u16) -> Option<Offset> {
        let face = self.face().ok()?;

        mark_to_base_offset_for_face(&face, to_ttf_glyph(base), to_ttf_glyph(mark), size_px)
    }

    fn mark_to_ligature_offset(
        &self,
        ligature: GlyphId,
        component: u16,
        mark: GlyphId,
        size_px: u16,
    ) -> Option<Offset> {
        let face = self.face().ok()?;

        mark_to_ligature_offset_for_face(
            &face,
            to_ttf_glyph(ligature),
            component,
            to_ttf_glyph(mark),
            size_px,
        )
    }

    fn mark_to_mark_offset(
        &self,
        base_mark: GlyphId,
        mark: GlyphId,
        size_px: u16,
    ) -> Option<Offset> {
        let face = self.face().ok()?;

        mark_to_mark_offset_for_face(&face, to_ttf_glyph(base_mark), to_ttf_glyph(mark), size_px)
    }

    fn rasterize(
        &self,
        glyph: GlyphId,
        size_px: u16,
        coverage: &mut [u8],
    ) -> Result<(), FontRasterError> {
        if size_px == 0 {
            return Err(FontRasterError::InvalidSize);
        }

        let face = self.face().map_err(|_| FontRasterError::InvalidFont)?;
        let glyph = to_ttf_glyph(glyph);
        let metrics =
            glyph_metrics_for_face(&face, glyph, size_px).ok_or(FontRasterError::InvalidGlyph)?;
        let required = metrics
            .coverage_bytes()
            .ok_or(FontRasterError::InvalidGlyph)?;

        if coverage.len() < required {
            return Err(FontRasterError::BufferTooSmall);
        }

        coverage[..required].fill(0);

        if required == 0 {
            // whitespace and otehr zero-outline glyphs are valid
            return Ok(());
        }

        let Some(scale) = font_scale(&face, size_px) else {
            return Err(FontRasterError::InvalidSize);
        };

        let width = usize::from(metrics.width);
        let height = usize::from(metrics.height);
        let left = metrics.bearing_x.get();
        let top = metrics.bearing_y.get();

        for row in 0..height {
            for sub_y in 0..SUPERSAMPLE_Y {
                let sample_y = row as f32 + (sub_y as f32 + 0.5) / SUPERSAMPLE_Y as f32;
                let mut scanline = ScanlineBuilder::new(scale, left, top, sample_y);

                if face.outline_glyph(glyph, &mut scanline).is_none() {
                    return Err(FontRasterError::Unsupported);
                }
                if scanline.overflowed() {
                    return Err(FontRasterError::OutlineTooComplex);
                }

                scanline.sort_intersections();

                accumulate_scanline(
                    width,
                    row,
                    &mut coverage[..required],
                    scanline.intersections(),
                );
            }
        }

        normalize_coverage(&mut coverage[..required]);

        Ok(())
    }
}

fn to_ttf_glyph(glyph: GlyphId) -> TtfGlyphId {
    TtfGlyphId(glyph.value())
}
