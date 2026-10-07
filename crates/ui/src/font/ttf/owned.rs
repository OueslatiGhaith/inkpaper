use crate::{
    CursiveAttachment, FontData, FontFace, FontMetrics, FontProperties, FontRasterError, FontStyle,
    FontWeightRange, GlyphId, GlyphMetrics, Offset, OpenTypeFeature, PairPositioning, Pixels,
    PreparedFontSource,
};

use super::{PreparedTtfFont, TtfFont, TtfFontError};

/// A [`TtfFont`] that keeps its bytes, like a font read from a file, so it
/// can be registered as an owned face. Shared bytes such as `Rc<[u8]>` let
/// several registries use one copy.
#[derive(Debug, Clone)]
pub struct OwnedTtfFont<D> {
    data: D,
    face_index: u32,
}

impl<D: AsRef<[u8]>> OwnedTtfFont<D> {
    /// Checks the bytes hold a font at `face_index`.
    pub fn parse(data: D, face_index: u32) -> Result<Self, TtfFontError> {
        TtfFont::parse(FontData::new(data.as_ref()), face_index)?;

        Ok(Self { data, face_index })
    }

    pub fn data(&self) -> &D {
        &self.data
    }

    fn font(&self) -> TtfFont<'_> {
        TtfFont::from_data(FontData::new(self.data.as_ref()), self.face_index)
    }
}

impl<D: AsRef<[u8]>> FontFace for OwnedTtfFont<D> {
    fn prepare_with_properties(&self, properties: FontProperties) -> PreparedFontSource<'_> {
        match self.font().face_with_properties(properties) {
            Ok(face) => PreparedFontSource::from_ttf(PreparedTtfFont::new(face)),
            Err(_) => PreparedFontSource::unprepared(),
        }
    }

    fn weight_range(&self) -> FontWeightRange {
        self.font().weight_range()
    }

    fn style(&self) -> FontStyle {
        self.font().style()
    }

    fn glyph_id(&self, character: char) -> Option<GlyphId> {
        self.font().glyph_id(character)
    }

    fn metrics(&self, size_px: u16) -> FontMetrics {
        self.font().metrics(size_px)
    }

    fn glyph_advance(&self, glyph: GlyphId, size_px: u16) -> Option<Pixels> {
        self.font().glyph_advance(glyph, size_px)
    }

    fn glyph_metrics(&self, glyph: GlyphId, size_px: u16) -> Option<GlyphMetrics> {
        self.font().glyph_metrics(glyph, size_px)
    }

    fn kerning(&self, left: GlyphId, right: GlyphId, size_px: u16) -> Pixels {
        self.font().kerning(left, right, size_px)
    }

    fn single_substitution(&self, feature: OpenTypeFeature, glyph: GlyphId) -> Option<GlyphId> {
        self.font().single_substitution(feature, glyph)
    }

    fn ligature_substitution(
        &self,
        feature: OpenTypeFeature,
        first: GlyphId,
        second: GlyphId,
    ) -> Option<GlyphId> {
        self.font().ligature_substitution(feature, first, second)
    }

    fn cursive_attachment(
        &self,
        visual_left: GlyphId,
        visual_right: GlyphId,
        size_px: u16,
        right_to_left: bool,
    ) -> Option<CursiveAttachment> {
        self.font()
            .cursive_attachment(visual_left, visual_right, size_px, right_to_left)
    }

    fn mark_to_base_offset(&self, base: GlyphId, mark: GlyphId, size_px: u16) -> Option<Offset> {
        self.font().mark_to_base_offset(base, mark, size_px)
    }

    fn mark_to_ligature_offset(
        &self,
        ligature: GlyphId,
        component: u16,
        mark: GlyphId,
        size_px: u16,
    ) -> Option<Offset> {
        self.font()
            .mark_to_ligature_offset(ligature, component, mark, size_px)
    }

    fn mark_to_mark_offset(
        &self,
        base_mark: GlyphId,
        mark: GlyphId,
        size_px: u16,
    ) -> Option<Offset> {
        self.font().mark_to_mark_offset(base_mark, mark, size_px)
    }

    fn rasterize(
        &self,
        glyph: GlyphId,
        size_px: u16,
        coverage: &mut [u8],
    ) -> Result<(), FontRasterError> {
        self.font().rasterize(glyph, size_px, coverage)
    }

    fn rasterize_with_properties(
        &self,
        properties: FontProperties,
        glyph: GlyphId,
        size_px: u16,
        coverage: &mut [u8],
    ) -> Result<(), FontRasterError> {
        self.font()
            .rasterize_with_properties(properties, glyph, size_px, coverage)
    }

    fn glyph_id_with_properties(
        &self,
        properties: FontProperties,
        character: char,
    ) -> Option<GlyphId> {
        self.font().glyph_id_with_properties(properties, character)
    }

    fn metrics_with_properties(&self, properties: FontProperties, size_px: u16) -> FontMetrics {
        self.font().metrics_with_properties(properties, size_px)
    }

    fn glyph_advance_with_properties(
        &self,
        properties: FontProperties,
        glyph: GlyphId,
        size_px: u16,
    ) -> Option<Pixels> {
        self.font()
            .glyph_advance_with_properties(properties, glyph, size_px)
    }

    fn glyph_id_and_advance_with_properties(
        &self,
        properties: FontProperties,
        character: char,
        size_px: u16,
    ) -> Option<(GlyphId, Pixels)> {
        self.font()
            .glyph_id_and_advance_with_properties(properties, character, size_px)
    }

    fn glyph_metrics_with_properties(
        &self,
        properties: FontProperties,
        glyph: GlyphId,
        size_px: u16,
    ) -> Option<GlyphMetrics> {
        self.font()
            .glyph_metrics_with_properties(properties, glyph, size_px)
    }

    fn kerning_with_properties(
        &self,
        properties: FontProperties,
        left: GlyphId,
        right: GlyphId,
        size_px: u16,
    ) -> Pixels {
        self.font()
            .kerning_with_properties(properties, left, right, size_px)
    }

    fn single_substitution_with_properties(
        &self,
        properties: FontProperties,
        feature: OpenTypeFeature,
        glyph: GlyphId,
    ) -> Option<GlyphId> {
        self.font()
            .single_substitution_with_properties(properties, feature, glyph)
    }

    fn ligature_substitution_with_properties(
        &self,
        properties: FontProperties,
        feature: OpenTypeFeature,
        first: GlyphId,
        second: GlyphId,
    ) -> Option<GlyphId> {
        self.font()
            .ligature_substitution_with_properties(properties, feature, first, second)
    }

    fn cursive_attachment_with_properties(
        &self,
        properties: FontProperties,
        visual_left: GlyphId,
        visual_right: GlyphId,
        size_px: u16,
        right_to_left: bool,
    ) -> Option<CursiveAttachment> {
        self.font().cursive_attachment_with_properties(
            properties,
            visual_left,
            visual_right,
            size_px,
            right_to_left,
        )
    }

    fn pair_positioning_with_properties(
        &self,
        properties: FontProperties,
        visual_left: GlyphId,
        visual_right: GlyphId,
        size_px: u16,
        right_to_left: bool,
    ) -> PairPositioning {
        self.font().pair_positioning_with_properties(
            properties,
            visual_left,
            visual_right,
            size_px,
            right_to_left,
        )
    }

    fn mark_to_base_offset_with_properties(
        &self,
        properties: FontProperties,
        base: GlyphId,
        mark: GlyphId,
        size_px: u16,
    ) -> Option<Offset> {
        self.font()
            .mark_to_base_offset_with_properties(properties, base, mark, size_px)
    }

    fn mark_to_ligature_offset_with_properties(
        &self,
        properties: FontProperties,
        ligature: GlyphId,
        component: u16,
        mark: GlyphId,
        size_px: u16,
    ) -> Option<Offset> {
        self.font()
            .mark_to_ligature_offset_with_properties(properties, ligature, component, mark, size_px)
    }

    fn mark_to_mark_offset_with_properties(
        &self,
        properties: FontProperties,
        base_mark: GlyphId,
        mark: GlyphId,
        size_px: u16,
    ) -> Option<Offset> {
        self.font()
            .mark_to_mark_offset_with_properties(properties, base_mark, mark, size_px)
    }
}
