use crate::{
    FontData, FontFace, FontMetrics, FontProperties, FontRasterError, FontStyle, FontWeightRange,
    GlyphId, GlyphMetrics, Pixels, PreparedFontSource,
};

#[cfg(feature = "hinting")]
use super::HintedOwnedTtfFont;
use super::{PreparedTtfFont, TtfFont, TtfFontError};

/// A [`TtfFont`] that keeps its bytes, like a font read from a file, so it
/// can be registered as an owned face. Shared bytes such as `Arc<[u8]>` let
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

    pub const fn face_index(&self) -> u32 {
        self.face_index
    }

    #[cfg(feature = "hinting")]
    pub fn into_hinted(self) -> HintedOwnedTtfFont<D>
    where
        D: Send + Sync + 'static,
    {
        HintedOwnedTtfFont::new(self)
    }

    #[cfg(feature = "hinting")]
    pub(super) fn into_parts(self) -> (D, u32) {
        (self.data, self.face_index)
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
                let harfrust = self.font().harfrust_font(properties);
                PreparedFontSource::from_ttf(PreparedTtfFont::new(font, location, harfrust))
            }
            Err(_) => PreparedFontSource::unprepared(),
        }
    }

    fn is_ttf_shaping(&self) -> bool {
        true
    }

    fn harfrust_font(&self, properties: FontProperties) -> Option<harfrust::Font> {
        self.font().harfrust_font(properties)
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
