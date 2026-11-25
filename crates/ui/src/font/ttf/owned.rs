use crate::{
    CursiveAttachment, FontData, FontFace, FontMetrics, FontProperties, FontRasterError, FontStyle,
    FontWeightRange, GlyphId, GlyphMetrics, Offset, OpenTypeFeature, PairPositioning, Pixels,
    PreparedFontSource,
};

#[cfg(feature = "hinting")]
use super::HintedOwnedTtfFont;
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

    #[cfg(feature = "hinting")]
    pub fn into_hinted(self) -> HintedOwnedTtfFont<D> {
        HintedOwnedTtfFont::new(self)
    }

    pub(super) fn font(&self) -> TtfFont<'_> {
        TtfFont::from_data(FontData::new(self.data.as_ref()), self.face_index)
    }
}

impl<D: AsRef<[u8]>> FontFace for OwnedTtfFont<D> {
    fn prepare(&self, properties: FontProperties) -> PreparedFontSource<'_> {
        match self.font().font_ref() {
            Ok(font) => {
                let location = TtfFont::location_for(&font, properties);
                PreparedFontSource::from_ttf(PreparedTtfFont::new(font, location))
            }
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

    fn metrics(&self, properties: FontProperties, size_px: u16) -> FontMetrics {
        self.font().metrics(properties, size_px)
    }

    fn glyph_advance(
        &self,
        properties: FontProperties,
        glyph: GlyphId,
        size_px: u16,
    ) -> Option<Pixels> {
        self.font().glyph_advance(properties, glyph, size_px)
    }

    fn glyph_id_and_advance(
        &self,
        properties: FontProperties,
        character: char,
        size_px: u16,
    ) -> Option<(GlyphId, Pixels)> {
        self.font()
            .glyph_id_and_advance(properties, character, size_px)
    }

    fn glyph_metrics(
        &self,
        properties: FontProperties,
        glyph: GlyphId,
        size_px: u16,
    ) -> Option<GlyphMetrics> {
        self.font().glyph_metrics(properties, glyph, size_px)
    }

    fn kerning(
        &self,
        properties: FontProperties,
        left: GlyphId,
        right: GlyphId,
        size_px: u16,
    ) -> Pixels {
        self.font().kerning(properties, left, right, size_px)
    }

    fn single_substitution(
        &self,
        properties: FontProperties,
        feature: OpenTypeFeature,
        glyph: GlyphId,
    ) -> Option<GlyphId> {
        self.font().single_substitution(properties, feature, glyph)
    }

    fn ligature_substitution(
        &self,
        properties: FontProperties,
        feature: OpenTypeFeature,
        first: GlyphId,
        second: GlyphId,
    ) -> Option<GlyphId> {
        self.font()
            .ligature_substitution(properties, feature, first, second)
    }

    fn cursive_attachment(
        &self,
        properties: FontProperties,
        visual_left: GlyphId,
        visual_right: GlyphId,
        size_px: u16,
        right_to_left: bool,
    ) -> Option<CursiveAttachment> {
        self.font()
            .cursive_attachment(properties, visual_left, visual_right, size_px, right_to_left)
    }

    fn pair_positioning(
        &self,
        properties: FontProperties,
        visual_left: GlyphId,
        visual_right: GlyphId,
        size_px: u16,
        right_to_left: bool,
    ) -> PairPositioning {
        self.font()
            .pair_positioning(properties, visual_left, visual_right, size_px, right_to_left)
    }

    fn mark_to_base_offset(
        &self,
        properties: FontProperties,
        base: GlyphId,
        mark: GlyphId,
        size_px: u16,
    ) -> Option<Offset> {
        self.font()
            .mark_to_base_offset(properties, base, mark, size_px)
    }

    fn mark_to_ligature_offset(
        &self,
        properties: FontProperties,
        ligature: GlyphId,
        component: u16,
        mark: GlyphId,
        size_px: u16,
    ) -> Option<Offset> {
        self.font()
            .mark_to_ligature_offset(properties, ligature, component, mark, size_px)
    }

    fn mark_to_mark_offset(
        &self,
        properties: FontProperties,
        base_mark: GlyphId,
        mark: GlyphId,
        size_px: u16,
    ) -> Option<Offset> {
        self.font()
            .mark_to_mark_offset(properties, base_mark, mark, size_px)
    }

    fn rasterize(
        &self,
        properties: FontProperties,
        glyph: GlyphId,
        size_px: u16,
        coverage: &mut [u8],
    ) -> Result<(), FontRasterError> {
        self.font().rasterize(properties, glyph, size_px, coverage)
    }
}
