use crate::{Offset, Pixels, px};

#[cfg(feature = "ttf")]
pub mod ttf;

mod cache;
mod registry;
mod resources;
#[cfg(test)]
mod tests;

pub use cache::{GlyphBitmap, GlyphCache, GlyphCacheError};
pub use registry::{FontRegistry, FontRegistryError, ResolvedGlyph};
pub use resources::FontResources;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct FontId(u16);

impl FontId {
    pub const DEFAULT: Self = Self(0);

    pub const fn new(value: u16) -> Self {
        Self(value)
    }

    pub(crate) const fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct GlyphId(u16);

impl GlyphId {
    pub const fn new(value: u16) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u16 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct OpenTypeFeature([u8; 4]);

impl OpenTypeFeature {
    pub const fn new(tag: [u8; 4]) -> Self {
        Self(tag)
    }

    pub const fn tag(self) -> [u8; 4] {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FontData<'a> {
    bytes: &'a [u8],
}

impl<'a> FontData<'a> {
    pub const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }

    pub const fn bytes(self) -> &'a [u8] {
        self.bytes
    }

    pub const fn len(self) -> usize {
        self.bytes.len()
    }

    pub const fn is_empty(self) -> bool {
        self.bytes.is_empty()
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct FontMetrics {
    pub ascent: Pixels,
    pub descent: Pixels,
    pub line_gap: Pixels,
}

impl FontMetrics {
    pub const fn new(ascent: Pixels, descent: Pixels, line_gap: Pixels) -> Self {
        Self {
            ascent,
            descent,
            line_gap,
        }
    }

    pub fn line_height(self) -> Pixels {
        self.ascent + self.descent + self.line_gap
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct GlyphMetrics {
    pub width: u16,
    pub height: u16,
    /// offset of the rasterized bitmap from the glyph's baseline pen position
    pub bearing_x: Pixels,
    pub bearing_y: Pixels,
    /// horizontal pen advance after drawing this glyph
    pub advance: Pixels,
}

impl GlyphMetrics {
    pub const fn new(
        width: u16,
        height: u16,
        bearing_x: Pixels,
        bearing_y: Pixels,
        advance: Pixels,
    ) -> Self {
        Self {
            width,
            height,
            bearing_x,
            bearing_y,
            advance,
        }
    }

    pub fn coverage_bytes(self) -> Option<usize> {
        usize::from(self.width).checked_mul(usize::from(self.height))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct CursiveAttachment {
    origin_delta: Offset,
}

impl CursiveAttachment {
    pub const fn new(origin_delta: Offset) -> Self {
        Self { origin_delta }
    }

    /// desired origin of the visual-right glyph relative to the origin of the visual-left glyph.
    pub const fn origin_delta(self) -> Offset {
        self.origin_delta
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum FontRasterError {
    InvalidFont,
    InvalidGlyph,
    InvalidSize,
    BufferTooSmall,
    OutlineTooComplex,
    Unsupported,
}

pub trait FontFace {
    /// resolve one unicode scalar value to a font-local glyph.
    fn glyph_id(&self, character: char) -> Option<GlyphId>;
    fn metrics(&self, size_px: u16) -> FontMetrics;

    /// return only the horizontal advance
    ///
    /// the default is convenient for bitmap fonts. Scalable fonts should override this
    /// because advnace lookup is substaintially cheaper than calculating the glyph's
    /// raster bounds
    fn glyph_advance(&self, glyph: GlyphId, size_px: u16) -> Option<Pixels> {
        self.glyph_metrics(glyph, size_px)
            .map(|metrics| metrics.advance)
    }

    fn glyph_metrics(&self, glyph: GlyphId, size_px: u16) -> Option<GlyphMetrics>;

    fn kerning(&self, _left: GlyphId, _right: GlyphId, _size_px: u16) -> Pixels {
        px(0)
    }

    fn single_substitution(&self, _feature: OpenTypeFeature, _glyph: GlyphId) -> Option<GlyphId> {
        None
    }

    fn ligature_substitution(
        &self,
        _feature: OpenTypeFeature,
        _first: GlyphId,
        _second: GlyphId,
    ) -> Option<GlyphId> {
        None
    }

    fn cursive_attachment(
        &self,
        _visual_left: GlyphId,
        _visual_right: GlyphId,
        _size_px: u16,
        _right_to_left: bool,
    ) -> Option<CursiveAttachment> {
        None
    }

    /// returns the child mark glyph origin relative to the base glyph origin,
    /// in framebuffer coordinates.
    ///
    /// the returned Y axis therefore grows downward even when the underlying font stores
    /// its anchor in the usual OpenType Y-up coodinate system.
    ///
    /// fonts without attachement metadata simply return `None`
    fn mark_to_base_offset(&self, _base: GlyphId, _mark: GlyphId, _size_px: u16) -> Option<Offset> {
        None
    }

    /// returns the child mark glyph origin relative to one logical component of a
    /// ligature glyph
    ///
    /// `component` is zero-based and follows the ligature's logical component order
    fn mark_to_ligature_offset(
        &self,
        _ligature: GlyphId,
        _component: u16,
        _mark: GlyphId,
        _size_px: u16,
    ) -> Option<Offset> {
        None
    }

    /// returns the child mark glyph origin relative to an already positioned parent
    /// mark glyph origin.
    ///
    /// this is the font-independent interface used for OpenType mark-to-mark attachement
    fn mark_to_mark_offset(
        &self,
        _base_mark: GlyphId,
        _mark: GlyphId,
        _size_px: u16,
    ) -> Option<Offset> {
        None
    }

    /// rasterize the glyph into an 8-bit coverage bitmap:
    ///
    /// coverage layout:
    /// ```txt
    ///     width * heigh bytes
    ///     row-major
    ///
    ///     0 = no coverage
    ///     255 = full covered
    /// ```
    fn rasterize(
        &self,
        glyph: GlyphId,
        size_px: u16,
        coverage: &mut [u8],
    ) -> Result<(), FontRasterError>;
}
