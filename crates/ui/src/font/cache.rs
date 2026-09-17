use crate::{FontInstance, increment_metric, px};

use super::{FontRasterError, GlyphId, GlyphMetrics, registry::FontRegistry};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GlyphCacheKey {
    font: FontInstance,
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
            font: FontInstance::DEFAULT,
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

#[cfg(feature = "metrics")]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct GlyphCacheMetrics {
    pub lookups: u64,
    pub hits: u64,
    pub misses: u64,
    pub collisions: u64,
    pub rasterizations: u64,
    pub clears: u64,
    pub bytes_peak: usize,
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

pub struct GlyphCache<const SLOTS: usize, const BYTES: usize> {
    storage: [u8; BYTES],
    used: usize,
    slots: [GlyphCacheSlot; SLOTS],
    #[cfg(feature = "metrics")]
    metrics: GlyphCacheMetrics,
}

impl<const SLOTS: usize, const BYTES: usize> Default for GlyphCache<SLOTS, BYTES> {
    fn default() -> Self {
        assert!(SLOTS > 0, "glyph cache must contain at least one slot");

        Self {
            storage: [0; BYTES],
            used: 0,
            slots: [GlyphCacheSlot::EMPTY; SLOTS],
            #[cfg(feature = "metrics")]
            metrics: GlyphCacheMetrics::default(),
        }
    }
}

impl<const SLOTS: usize, const BYTES: usize> GlyphCache<SLOTS, BYTES> {
    pub const fn capacity_bytes(&self) -> usize {
        BYTES
    }

    pub const fn used_bytes(&self) -> usize {
        self.used
    }

    pub const fn remaining_bytes(&self) -> usize {
        BYTES.saturating_sub(self.used)
    }

    pub fn clear(&mut self) {
        increment_metric!(self.metrics.clears);

        self.used = 0;
        self.slots.fill(GlyphCacheSlot::EMPTY);
    }

    #[cfg(feature = "metrics")]
    pub fn reset_metrics(&mut self) {
        self.metrics = GlyphCacheMetrics {
            bytes_peak: self.used,
            ..GlyphCacheMetrics::default()
        };
    }

    #[cfg(feature = "metrics")]
    pub const fn metrics(&self) -> GlyphCacheMetrics {
        self.metrics
    }

    fn slot_index(key: GlyphCacheKey) -> usize {
        let font = key.font.font().index();
        let weight = usize::from(key.font.weight().value());
        let glyph = usize::from(key.glyph.value());
        let size = usize::from(key.size_px);

        font.wrapping_mul(31)
            .wrapping_add(weight.wrapping_mul(13))
            .wrapping_add(glyph.wrapping_mul(17))
            .wrapping_add(size)
            % SLOTS
    }

    pub fn get_or_rasterize<const FONTS: usize>(
        &mut self,
        registry: &FontRegistry<'_, FONTS>,
        font: impl Into<FontInstance>,
        glyph: GlyphId,
        size_px: u16,
    ) -> Result<GlyphBitmap<'_>, GlyphCacheError> {
        let font = registry
            .resolve_instance(font.into())
            .ok_or(GlyphCacheError::MissingFont)?;

        let key = GlyphCacheKey {
            font: font.instance(),
            glyph,
            size_px,
        };

        let home_slot = Self::slot_index(key);
        let mut insertion_slot = None;

        increment_metric!(self.metrics.lookups);

        // entries are never individually deleted, so encountering the first empty slot
        // proves that the key is not present later in the probe sequence.
        for probe in 0..SLOTS {
            let slot_index = (home_slot + probe) % SLOTS;
            let slot = self.slots[slot_index];

            if !slot.valid {
                insertion_slot = Some(slot_index);
                break;
            }

            if slot.key == key {
                increment_metric!(self.metrics.hits);

                return Ok(GlyphBitmap {
                    coverage: &self.storage[slot.offset..slot.offset + slot.len],
                    metrics: slot.metrics,
                });
            }
        }

        increment_metric!(self.metrics.misses);

        // keep the collision metric comparable with the old direct-mapped cache:
        // count a miss as a collision when its ideal slot was already occupied.
        #[cfg(feature = "metrics")]
        if self.slots[home_slot].valid {
            increment_metric!(self.metrics.collisions);
        }

        let metrics = font
            .glyph_metrics(glyph, size_px)
            .ok_or(GlyphCacheError::MissingGlyph)?;
        let required = metrics
            .coverage_bytes()
            .ok_or(GlyphCacheError::GlyphTooLarge)?;

        if required > BYTES {
            return Err(GlyphCacheError::GlyphTooLarge);
        }

        // there are two reasons to start a fresh generation:
        //
        // 1. the metadata table has no free slot;
        // 2. the bitmap arena has insufficient remaining storage.
        //
        // after clearing, every metadata slot is free and the entire byte arena is
        // available, so the glyph can go directly into its home slot.
        let slot_index = if insertion_slot.is_none() || required > self.remaining_bytes() {
            self.clear();
            home_slot
        } else {
            insertion_slot.expect("a non-full glyph cache must have an insertion slot")
        };

        let offset = self.used;
        let end = offset
            .checked_add(required)
            .ok_or(GlyphCacheError::GlyphTooLarge)?;

        font.rasterize(glyph, size_px, &mut self.storage[offset..end])
            .map_err(GlyphCacheError::Raster)?;

        increment_metric!(self.metrics.rasterizations);

        self.used = end;

        #[cfg(feature = "metrics")]
        {
            self.metrics.bytes_peak = self.metrics.bytes_peak.max(self.used);
        }

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
