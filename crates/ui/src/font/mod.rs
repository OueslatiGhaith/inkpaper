use crate::{Pixels, px};

#[cfg(feature = "ttf")]
pub mod ttf;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum FontRegistryError {
    Full,
}

pub struct FontRegistry<'font, const FONTS: usize> {
    fonts: [Option<&'font dyn FontFace>; FONTS],
    len: usize,
}

impl<const FONTS: usize> Default for FontRegistry<'_, FONTS> {
    fn default() -> Self {
        Self {
            fonts: [None; FONTS],
            len: 0,
        }
    }
}

impl<'font, const FONTS: usize> FontRegistry<'font, FONTS> {
    pub const fn len(&self) -> usize {
        self.len
    }

    pub const fn capacity(&self) -> usize {
        FONTS
    }

    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn register(&mut self, font: &'font dyn FontFace) -> Result<FontId, FontRegistryError> {
        if self.len >= FONTS {
            return Err(FontRegistryError::Full);
        }

        let index = u16::try_from(self.len).map_err(|_| FontRegistryError::Full)?;

        self.fonts[self.len] = Some(font);
        self.len += 1;

        Ok(FontId::new(index))
    }

    pub fn get(&self, id: FontId) -> Option<&'font dyn FontFace> {
        self.fonts.get(id.index()).copied().flatten()
    }

    pub fn default_font(&self) -> Option<&'font dyn FontFace> {
        self.get(FontId::DEFAULT)
    }

    pub fn resolve_id(&self, id: FontId) -> Option<FontId> {
        if self.get(id).is_some() {
            Some(id)
        } else if self.default_font().is_some() {
            Some(FontId::DEFAULT)
        } else {
            None
        }
    }

    pub fn resolve_with_id(&self, id: FontId) -> Option<(FontId, &'font dyn FontFace)> {
        let resolved = self.resolve_id(id)?;
        let font = self.get(resolved)?;

        Some((resolved, font))
    }

    pub fn resolve(&self, id: FontId) -> Option<&'font dyn FontFace> {
        self.resolve_with_id(id).map(|(_, font)| font)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GlyphCacheKey {
    font: FontId,
    glyph: GlyphId,
    size_px: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GlyphCacheSlot {
    valid: bool,
    key: GlyphCacheKey,
    offset: usize,
    len: usize,
    metrics: GlyphMetrics,
}

impl GlyphCacheSlot {
    const EMPTY: Self = Self {
        valid: false,
        key: GlyphCacheKey {
            font: FontId::DEFAULT,
            glyph: GlyphId::new(0),
            size_px: 0,
        },
        offset: 0,
        len: 0,
        metrics: GlyphMetrics {
            width: 0,
            height: 0,
            bearing_x: px(0),
            bearing_y: px(0),
            advance: px(0),
        },
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum GlyphCacheError {
    MissingFont,
    MissingGlyph,
    GlyphTooLarge,
    Raster(FontRasterError),
}

pub struct GlyphBitmap<'a> {
    coverage: &'a [u8],
    metrics: GlyphMetrics,
}

impl<'a> GlyphBitmap<'a> {
    pub const fn coverage(&self) -> &'a [u8] {
        self.coverage
    }

    pub const fn metrics(&self) -> GlyphMetrics {
        self.metrics
    }

    pub const fn width(&self) -> u16 {
        self.metrics.width
    }

    pub const fn height(&self) -> u16 {
        self.metrics.height
    }
}

pub struct GlyphCache<'storage, const SLOTS: usize> {
    storage: &'storage mut [u8],
    used: usize,
    slots: [GlyphCacheSlot; SLOTS],
}

impl<'storage, const SLOTS: usize> GlyphCache<'storage, SLOTS> {
    pub fn new(storage: &'storage mut [u8]) -> Self {
        assert!(SLOTS > 0, "glyph cache must contain at least one slot");

        Self {
            storage,
            used: 0,
            slots: [GlyphCacheSlot::EMPTY; SLOTS],
        }
    }

    pub const fn capacity_bytes(&self) -> usize {
        self.storage.len()
    }

    pub const fn used_bytes(&self) -> usize {
        self.used
    }

    pub const fn remaining_bytes(&self) -> usize {
        self.storage.len().saturating_sub(self.used)
    }

    pub fn clear(&mut self) {
        self.used = 0;

        self.slots.fill(GlyphCacheSlot::EMPTY);
    }

    fn slot_index(key: GlyphCacheKey) -> usize {
        let font = key.font.index();
        let glyph = usize::from(key.glyph.value());
        let size = usize::from(key.size_px);

        font.wrapping_mul(31)
            .wrapping_add(glyph.wrapping_mul(17))
            .wrapping_add(size)
            % SLOTS
    }

    fn cached_metadata(&self, key: GlyphCacheKey) -> Option<(usize, usize, GlyphMetrics)> {
        let slot = self.slots[Self::slot_index(key)];
        if !slot.valid || slot.key != key {
            return None;
        }

        Some((slot.offset, slot.len, slot.metrics))
    }

    pub fn get_or_rasterize<const FONTS: usize>(
        &mut self,
        registry: &FontRegistry<'_, FONTS>,
        font: FontId,
        glyph: GlyphId,
        size_px: u16,
    ) -> Result<GlyphBitmap<'_>, GlyphCacheError> {
        let key = GlyphCacheKey {
            font,
            glyph,
            size_px,
        };

        if let Some((offset, len, metrics)) = self.cached_metadata(key) {
            return Ok(GlyphBitmap {
                coverage: &self.storage[offset..offset + len],
                metrics,
            });
        };

        let face = registry.resolve(font).ok_or(GlyphCacheError::MissingFont)?;
        let metrics = face
            .glyph_metrics(glyph, size_px)
            .ok_or(GlyphCacheError::MissingGlyph)?;
        let required = metrics
            .coverage_bytes()
            .ok_or(GlyphCacheError::GlyphTooLarge)?;

        if required > self.storage.len() {
            return Err(GlyphCacheError::GlyphTooLarge);
        }
        if required > self.remaining_bytes() {
            self.clear();
        }

        let offset = self.used;
        let end = offset
            .checked_add(required)
            .ok_or(GlyphCacheError::GlyphTooLarge)?;

        face.rasterize(glyph, size_px, &mut self.storage[offset..end])
            .map_err(GlyphCacheError::Raster)?;

        self.used = end;

        let slot_index = Self::slot_index(key);
        self.slots[slot_index] = GlyphCacheSlot {
            valid: true,
            key,
            offset,
            len: required,
            metrics,
        };

        Ok(GlyphBitmap {
            coverage: &self.storage[offset..end],
            metrics,
        })
    }
}

pub struct FontResources<'font, 'storage, const FONTS: usize, const GLYPH_SLOTS: usize> {
    registry: FontRegistry<'font, FONTS>,
    cache: GlyphCache<'storage, GLYPH_SLOTS>,
}

impl<'font, 'storage, const FONTS: usize, const GLYPH_SLOTS: usize>
    FontResources<'font, 'storage, FONTS, GLYPH_SLOTS>
{
    pub fn new(glyph_storage: &'storage mut [u8]) -> Self {
        Self {
            registry: FontRegistry::default(),
            cache: GlyphCache::new(glyph_storage),
        }
    }

    pub const fn len(&self) -> usize {
        self.registry.len()
    }

    pub const fn is_empty(&self) -> bool {
        self.registry.is_empty()
    }

    pub fn register(&mut self, font: &'font dyn FontFace) -> Result<FontId, FontRegistryError> {
        self.registry.register(font)
    }

    pub fn resolve(&self, id: FontId) -> Option<(FontId, &'font dyn FontFace)> {
        self.registry.resolve_with_id(id)
    }

    pub fn glyph_bitmap(
        &mut self,
        font: FontId,
        glyph: GlyphId,
        size_px: u16,
    ) -> Result<GlyphBitmap<'_>, GlyphCacheError> {
        let resolved = self
            .registry
            .resolve_id(font)
            .ok_or(GlyphCacheError::MissingFont)?;

        self.cache
            .get_or_rasterize(&self.registry, resolved, glyph, size_px)
    }

    pub fn clear_glyph_cache(&mut self) {
        self.cache.clear();
    }

    pub const fn glyph_cache_capacity_bytes(&self) -> usize {
        self.cache.capacity_bytes()
    }

    pub const fn glyph_cache_used_bytes(&self) -> usize {
        self.cache.used_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::px;

    struct TestFont {
        fill: u8,
    }

    impl FontFace for TestFont {
        fn glyph_id(&self, character: char) -> Option<GlyphId> {
            let value = u32::from(character);

            u16::try_from(value).ok().map(GlyphId::new)
        }

        fn metrics(&self, size_px: u16) -> FontMetrics {
            FontMetrics::new(px(i32::from(size_px)), px(0), px(0))
        }

        fn glyph_metrics(&self, _glyph: GlyphId, _size_px: u16) -> Option<GlyphMetrics> {
            Some(GlyphMetrics::new(2, 2, px(0), px(-2), px(3)))
        }

        fn rasterize(
            &self,
            _glyph: GlyphId,
            _size_px: u16,
            coverage: &mut [u8],
        ) -> Result<(), FontRasterError> {
            if coverage.len() < 4 {
                return Err(FontRasterError::BufferTooSmall);
            }

            coverage[..4].fill(self.fill);

            Ok(())
        }
    }

    #[test]
    fn registry_assigns_stable_font_ids() {
        let first = TestFont { fill: 10 };
        let second = TestFont { fill: 20 };

        let mut registry = FontRegistry::<2>::default();

        let first_id = registry.register(&first).unwrap();
        let second_id = registry.register(&second).unwrap();

        assert_eq!(first_id, FontId::DEFAULT);
        assert_eq!(second_id, FontId::new(1));
        assert_eq!(registry.len(), 2);
    }

    #[test]
    fn registry_reports_capacity_exhaustion() {
        let first = TestFont { fill: 10 };
        let second = TestFont { fill: 20 };

        let mut registry = FontRegistry::<1>::default();
        registry.register(&first).unwrap();

        assert_eq!(registry.register(&second), Err(FontRegistryError::Full));
    }

    #[test]
    fn glyph_cache_returns_cached_coverage() {
        let mut registry = FontRegistry::<1>::default();

        let font = TestFont { fill: 173 };
        let font_id = registry.register(&font).unwrap();
        let glyph = font.glyph_id('A').unwrap();

        let mut storage = [0u8; 16];
        let mut cache = GlyphCache::<4>::new(&mut storage);

        {
            let bitmap = cache
                .get_or_rasterize(&registry, font_id, glyph, 16)
                .unwrap();

            assert_eq!(bitmap.coverage(), &[173, 173, 173, 173]);
        }

        assert_eq!(cache.used_bytes(), 4);

        {
            let bitmap = cache
                .get_or_rasterize(&registry, font_id, glyph, 16)
                .unwrap();

            assert_eq!(bitmap.coverage(), &[173, 173, 173, 173]);
        }

        assert_eq!(cache.used_bytes(), 4);
    }

    #[test]
    fn glyph_cache_flushes_when_storage_is_full() {
        let mut registry = FontRegistry::<1>::default();

        let font = TestFont { fill: 99 };
        let font_id = registry.register(&font).unwrap();
        let mut storage = [0u8; 6];
        let mut cache = GlyphCache::<4>::new(&mut storage);

        let first = font.glyph_id('A').unwrap();
        let second = font.glyph_id('B').unwrap();

        cache
            .get_or_rasterize(&registry, font_id, first, 16)
            .unwrap();

        assert_eq!(cache.used_bytes(), 4);

        cache
            .get_or_rasterize(&registry, font_id, second, 16)
            .unwrap();

        // the second 4-byte glyph cannot fit after the first,so the cache resets
        // and begins a fresh generation.
        assert_eq!(cache.used_bytes(), 4);
    }

    #[test]
    fn glyph_larger_than_entire_cache_is_rejected() {
        let mut registry = FontRegistry::<1>::default();

        let font = TestFont { fill: 255 };
        let font_id = registry.register(&font).unwrap();
        let glyph = font.glyph_id('A').unwrap();

        let mut storage = [0u8; 3];
        let mut cache = GlyphCache::<4>::new(&mut storage);

        let result = cache.get_or_rasterize(&registry, font_id, glyph, 16);

        assert!(matches!(result, Err(GlyphCacheError::GlyphTooLarge)));
    }
}
