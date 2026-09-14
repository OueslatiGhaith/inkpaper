use crate::{Offset, Pixels, px};

mod cache;
mod measure;
mod registry;
mod resources;

#[cfg(feature = "ttf")]
pub mod ttf;

#[cfg(test)]
mod tests;

pub use cache::{GlyphBitmap, GlyphCache, GlyphCacheError};
pub use registry::{FontRegistry, FontRegistryError, ResolvedGlyph};
pub use resources::FontResources;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct FontFamilyId(u16);

impl FontFamilyId {
    pub const DEFAULT: Self = Self(0);

    pub const fn new(value: u16) -> Self {
        Self(value)
    }

    pub(crate) const fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct FontWeight(u16);

impl FontWeight {
    pub const THIN: Self = Self(100);
    pub const EXTRA_LIGHT: Self = Self(200);
    pub const LIGHT: Self = Self(300);
    pub const NORMAL: Self = Self(400);
    pub const MEDIUM: Self = Self(500);
    pub const SEMIBOLD: Self = Self(600);
    pub const BOLD: Self = Self(700);
    pub const EXTRA_BOLD: Self = Self(800);
    pub const BLACK: Self = Self(900);

    pub const fn new(value: u16) -> Self {
        Self(if value < 1 {
            1
        } else if value > 1000 {
            1000
        } else {
            value
        })
    }

    pub const fn value(self) -> u16 {
        self.0
    }

    pub(crate) const fn distance(self, other: Self) -> u16 {
        self.0.abs_diff(other.0)
    }
}

impl Default for FontWeight {
    fn default() -> Self {
        Self::NORMAL
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct FontWeightRange {
    min: FontWeight,
    default: FontWeight,
    max: FontWeight,
}

impl FontWeightRange {
    pub const fn exact(weight: FontWeight) -> Self {
        Self {
            min: weight,
            default: weight,
            max: weight,
        }
    }

    pub const fn new(min: FontWeight, default: FontWeight, max: FontWeight) -> Self {
        let (min, max) = if min.value() <= max.value() {
            (min, max)
        } else {
            (max, min)
        };

        let default = if default.value() < min.value() {
            min
        } else if default.value() > max.value() {
            max
        } else {
            default
        };

        Self { min, default, max }
    }

    pub const fn min(self) -> FontWeight {
        self.min
    }

    pub const fn default_weight(self) -> FontWeight {
        self.default
    }

    pub const fn max(self) -> FontWeight {
        self.max
    }

    pub const fn contains(self, weight: FontWeight) -> bool {
        weight.value() >= self.min.value() && weight.value() <= self.max.value()
    }

    pub const fn is_exact(self) -> bool {
        self.min.value() == self.max.value()
    }

    pub const fn resolve(self, requested: FontWeight) -> FontWeight {
        if requested.value() < self.min.value() {
            self.min
        } else if requested.value() > self.max.value() {
            self.max
        } else {
            requested
        }
    }

    pub(crate) const fn distance(self, requested: FontWeight) -> u16 {
        requested.distance(self.resolve(requested))
    }
}

impl Default for FontWeightRange {
    fn default() -> Self {
        Self::exact(FontWeight::NORMAL)
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct FontProperties {
    weight: FontWeight,
}

impl FontProperties {
    pub const NORMAL: Self = Self {
        weight: FontWeight::NORMAL,
    };

    pub const fn new(weight: FontWeight) -> Self {
        Self { weight }
    }

    pub const fn weight(self) -> FontWeight {
        self.weight
    }
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct FontInstance {
    font: FontId,
    properties: FontProperties,
}

impl FontInstance {
    pub const DEFAULT: Self = Self::normal(FontId::DEFAULT);

    pub const fn new(font: FontId, properties: FontProperties) -> Self {
        Self { font, properties }
    }

    pub const fn normal(font: FontId) -> Self {
        Self::new(font, FontProperties::NORMAL)
    }

    pub const fn font(self) -> FontId {
        self.font
    }

    pub const fn properties(self) -> FontProperties {
        self.properties
    }

    pub const fn weight(self) -> FontWeight {
        self.properties.weight()
    }
}

impl Default for FontInstance {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl From<FontId> for FontInstance {
    fn from(font: FontId) -> Self {
        Self::normal(font)
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
    fn weight_range(&self) -> FontWeightRange {
        FontWeightRange::default()
    }

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

    fn glyph_id_with_properties(
        &self,
        _properties: FontProperties,
        character: char,
    ) -> Option<GlyphId> {
        self.glyph_id(character)
    }

    fn metrics_with_properties(&self, _properties: FontProperties, size_px: u16) -> FontMetrics {
        self.metrics(size_px)
    }

    fn glyph_advance_with_properties(
        &self,
        _properties: FontProperties,
        glyph: GlyphId,
        size_px: u16,
    ) -> Option<Pixels> {
        self.glyph_advance(glyph, size_px)
    }

    fn glyph_metrics_with_properties(
        &self,
        _properties: FontProperties,
        glyph: GlyphId,
        size_px: u16,
    ) -> Option<GlyphMetrics> {
        self.glyph_metrics(glyph, size_px)
    }

    fn kerning_with_properties(
        &self,
        _properties: FontProperties,
        left: GlyphId,
        right: GlyphId,
        size_px: u16,
    ) -> Pixels {
        self.kerning(left, right, size_px)
    }

    fn single_substitution_with_properties(
        &self,
        _properties: FontProperties,
        feature: OpenTypeFeature,
        glyph: GlyphId,
    ) -> Option<GlyphId> {
        self.single_substitution(feature, glyph)
    }

    fn ligature_substitution_with_properties(
        &self,
        _properties: FontProperties,
        feature: OpenTypeFeature,
        first: GlyphId,
        second: GlyphId,
    ) -> Option<GlyphId> {
        self.ligature_substitution(feature, first, second)
    }

    fn cursive_attachment_with_properties(
        &self,
        _properties: FontProperties,
        visual_left: GlyphId,
        visual_right: GlyphId,
        size_px: u16,
        right_to_left: bool,
    ) -> Option<CursiveAttachment> {
        self.cursive_attachment(visual_left, visual_right, size_px, right_to_left)
    }

    fn mark_to_base_offset_with_properties(
        &self,
        _properties: FontProperties,
        base: GlyphId,
        mark: GlyphId,
        size_px: u16,
    ) -> Option<Offset> {
        self.mark_to_base_offset(base, mark, size_px)
    }

    fn mark_to_ligature_offset_with_properties(
        &self,
        _properties: FontProperties,
        ligature: GlyphId,
        component: u16,
        mark: GlyphId,
        size_px: u16,
    ) -> Option<Offset> {
        self.mark_to_ligature_offset(ligature, component, mark, size_px)
    }

    fn mark_to_mark_offset_with_properties(
        &self,
        _properties: FontProperties,
        base_mark: GlyphId,
        mark: GlyphId,
        size_px: u16,
    ) -> Option<Offset> {
        self.mark_to_mark_offset(base_mark, mark, size_px)
    }

    fn rasterize_with_properties(
        &self,
        _properties: FontProperties,
        glyph: GlyphId,
        size_px: u16,
        coverage: &mut [u8],
    ) -> Result<(), FontRasterError> {
        self.rasterize(glyph, size_px, coverage)
    }
}

#[derive(Clone, Copy)]
pub struct ResolvedFont<'font> {
    instance: FontInstance,
    face: &'font dyn FontFace,
}

impl<'font> ResolvedFont<'font> {
    pub(crate) const fn new(instance: FontInstance, face: &'font dyn FontFace) -> Self {
        Self { instance, face }
    }

    pub const fn instance(self) -> FontInstance {
        self.instance
    }

    pub const fn id(self) -> FontId {
        self.instance.font()
    }

    pub const fn properties(self) -> FontProperties {
        self.instance.properties()
    }

    pub const fn weight(self) -> FontWeight {
        self.instance.weight()
    }

    pub const fn face(self) -> &'font dyn FontFace {
        self.face
    }

    pub fn glyph_id(self, character: char) -> Option<GlyphId> {
        self.face
            .glyph_id_with_properties(self.properties(), character)
    }

    pub fn metrics(self, size_px: u16) -> FontMetrics {
        self.face
            .metrics_with_properties(self.properties(), size_px)
    }

    pub fn glyph_advance(self, glyph: GlyphId, size_px: u16) -> Option<Pixels> {
        self.face
            .glyph_advance_with_properties(self.properties(), glyph, size_px)
    }

    pub fn glyph_metrics(self, glyph: GlyphId, size_px: u16) -> Option<GlyphMetrics> {
        self.face
            .glyph_metrics_with_properties(self.properties(), glyph, size_px)
    }

    pub fn kerning(self, left: GlyphId, right: GlyphId, size_px: u16) -> Pixels {
        self.face
            .kerning_with_properties(self.properties(), left, right, size_px)
    }

    pub fn single_substitution(self, feature: OpenTypeFeature, glyph: GlyphId) -> Option<GlyphId> {
        self.face
            .single_substitution_with_properties(self.properties(), feature, glyph)
    }

    pub fn ligature_substitution(
        self,
        feature: OpenTypeFeature,
        first: GlyphId,
        second: GlyphId,
    ) -> Option<GlyphId> {
        self.face
            .ligature_substitution_with_properties(self.properties(), feature, first, second)
    }

    pub fn cursive_attachment(
        self,
        visual_left: GlyphId,
        visual_right: GlyphId,
        size_px: u16,
        right_to_left: bool,
    ) -> Option<CursiveAttachment> {
        self.face.cursive_attachment_with_properties(
            self.properties(),
            visual_left,
            visual_right,
            size_px,
            right_to_left,
        )
    }

    pub fn mark_to_base_offset(self, base: GlyphId, mark: GlyphId, size_px: u16) -> Option<Offset> {
        self.face
            .mark_to_base_offset_with_properties(self.properties(), base, mark, size_px)
    }

    pub fn mark_to_ligature_offset(
        self,
        ligature: GlyphId,
        component: u16,
        mark: GlyphId,
        size_px: u16,
    ) -> Option<Offset> {
        self.face.mark_to_ligature_offset_with_properties(
            self.properties(),
            ligature,
            component,
            mark,
            size_px,
        )
    }

    pub fn mark_to_mark_offset(
        self,
        base_mark: GlyphId,
        mark: GlyphId,
        size_px: u16,
    ) -> Option<Offset> {
        self.face
            .mark_to_mark_offset_with_properties(self.properties(), base_mark, mark, size_px)
    }

    pub fn rasterize(
        self,
        glyph: GlyphId,
        size_px: u16,
        coverage: &mut [u8],
    ) -> Result<(), FontRasterError> {
        self.face
            .rasterize_with_properties(self.properties(), glyph, size_px, coverage)
    }
}
