use crate::px;

use super::{FontId, FontRasterError, GlyphId, GlyphMetrics, registry::FontRegistry};

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
