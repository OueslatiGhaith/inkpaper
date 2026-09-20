use crate::{FontInstance, FontRegistry, GlyphId, PairPositioning, increment_metric, px};

pub(crate) const PAIR_POSITIONING_CACHE_SLOTS: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PairPositioningCacheKey {
    font: FontInstance,
    visual_left: GlyphId,
    visual_right: GlyphId,
    size_px: u16,
    right_to_left: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PairPositioningCacheEntry {
    key: PairPositioningCacheKey,
    value: PairPositioning,
}

#[cfg(feature = "metrics")]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PairPositioningCacheMetrics {
    pub(crate) lookups: u64,
    pub(crate) hits: u64,
    pub(crate) misses: u64,
    pub(crate) collisions: u64,
}

#[cfg(feature = "metrics")]
impl PairPositioningCacheMetrics {
    pub(crate) fn delta_since(self, previous: Self) -> Self {
        Self {
            lookups: self.lookups.saturating_sub(previous.lookups),
            hits: self.hits.saturating_sub(previous.hits),
            misses: self.misses.saturating_sub(previous.misses),
            collisions: self.collisions.saturating_sub(previous.collisions),
        }
    }
}

pub(crate) struct PairPositioningCache<const SLOTS: usize> {
    slots: [Option<PairPositioningCacheEntry>; SLOTS],

    #[cfg(feature = "metrics")]
    metrics: PairPositioningCacheMetrics,
}

impl<const SLOTS: usize> Default for PairPositioningCache<SLOTS> {
    fn default() -> Self {
        assert!(
            SLOTS > 0,
            "pair positioning cache must contain at least one slot",
        );

        Self {
            slots: [None; SLOTS],

            #[cfg(feature = "metrics")]
            metrics: PairPositioningCacheMetrics::default(),
        }
    }
}

impl<const SLOTS: usize> PairPositioningCache<SLOTS> {
    fn slot_index(key: PairPositioningCacheKey) -> usize {
        let font = key.font.font().index();
        let weight = usize::from(key.font.weight().value());
        let left = usize::from(key.visual_left.value());
        let right = usize::from(key.visual_right.value());
        let size = usize::from(key.size_px);
        let direction = usize::from(u8::from(key.right_to_left));

        font.wrapping_mul(31)
            .wrapping_add(weight.wrapping_mul(13))
            .wrapping_add(left.wrapping_mul(17))
            .wrapping_add(right.wrapping_mul(37))
            .wrapping_add(size.wrapping_mul(7))
            .wrapping_add(direction.wrapping_mul(53))
            % SLOTS
    }

    fn get_or_insert_with(
        &mut self,
        key: PairPositioningCacheKey,
        compute: impl FnOnce() -> PairPositioning,
    ) -> PairPositioning {
        increment_metric!(self.metrics.lookups);

        let home_slot = Self::slot_index(key);
        let mut insertion_slot = None;

        // Entries are never deleted during this cache's lifetime, so the first empty
        // slot proves the key cannot exist later in the probe sequence.
        for probe in 0..SLOTS {
            let slot_index = (home_slot + probe) % SLOTS;

            match self.slots[slot_index] {
                Some(entry) if entry.key == key => {
                    increment_metric!(self.metrics.hits);

                    return entry.value;
                }

                Some(_) => {}

                None => {
                    insertion_slot = Some(slot_index);
                    break;
                }
            }
        }

        increment_metric!(self.metrics.misses);

        // Keep this metric comparable with the direct-mapped experiment:
        // a miss is considered a collision when its ideal slot was occupied.
        #[cfg(feature = "metrics")]
        if self.slots[home_slot].is_some() {
            increment_metric!(self.metrics.collisions);
        }

        let value = compute();

        // If there is still free capacity, use the first empty slot in the probe
        // sequence. Once the table is completely full, fall back to replacing the
        // home slot. A full table contains no empty terminators, so subsequent
        // lookups still scan the complete probe sequence correctly.
        let slot_index = insertion_slot.unwrap_or(home_slot);

        self.slots[slot_index] = Some(PairPositioningCacheEntry { key, value });

        value
    }

    pub(crate) fn get_or_compute<const FONTS: usize>(
        &mut self,
        registry: &FontRegistry<'_, FONTS>,
        font: FontInstance,
        visual_left: GlyphId,
        visual_right: GlyphId,
        size_px: u16,
        right_to_left: bool,
    ) -> PairPositioning {
        let key = PairPositioningCacheKey {
            font,
            visual_left,
            visual_right,
            size_px,
            right_to_left,
        };

        self.get_or_insert_with(key, || {
            registry
                .resolve_instance(font)
                .map(|face| {
                    face.pair_positioning(visual_left, visual_right, size_px, right_to_left)
                })
                .unwrap_or(PairPositioning::Kerning(px(0)))
        })
    }

    #[cfg(feature = "metrics")]
    pub(crate) const fn metrics(&self) -> PairPositioningCacheMetrics {
        self.metrics
    }
}

#[cfg(test)]
mod tests {
    use core::cell::Cell;

    use super::*;

    fn key(left: u16, right: u16) -> PairPositioningCacheKey {
        PairPositioningCacheKey {
            font: FontInstance::DEFAULT,
            visual_left: GlyphId::new(left),
            visual_right: GlyphId::new(right),
            size_px: 16,
            right_to_left: false,
        }
    }

    #[test]
    fn repeated_pair_uses_cached_value() {
        let mut cache = PairPositioningCache::<8>::default();
        let computes = Cell::new(0u32);

        let first = cache.get_or_insert_with(key(1, 2), || {
            computes.set(computes.get() + 1);
            PairPositioning::Kerning(px(3))
        });

        let second = cache.get_or_insert_with(key(1, 2), || {
            computes.set(computes.get() + 1);
            PairPositioning::Kerning(px(99))
        });

        assert_eq!(first, PairPositioning::Kerning(px(3)));
        assert_eq!(second, PairPositioning::Kerning(px(3)));
        assert_eq!(computes.get(), 1);
    }

    #[test]
    fn collision_replaces_direct_mapped_entry() {
        let mut cache = PairPositioningCache::<1>::default();
        let computes = Cell::new(0u32);

        let _ = cache.get_or_insert_with(key(1, 2), || {
            computes.set(computes.get() + 1);
            PairPositioning::Kerning(px(3))
        });

        let second = cache.get_or_insert_with(key(3, 4), || {
            computes.set(computes.get() + 1);
            PairPositioning::Kerning(px(7))
        });

        let first_again = cache.get_or_insert_with(key(1, 2), || {
            computes.set(computes.get() + 1);
            PairPositioning::Kerning(px(11))
        });

        assert_eq!(second, PairPositioning::Kerning(px(7)));
        assert_eq!(first_again, PairPositioning::Kerning(px(11)));
        assert_eq!(computes.get(), 3);
    }

    #[cfg(feature = "metrics")]
    #[test]
    fn metrics_distinguish_hits_misses_and_collisions() {
        let mut cache = PairPositioningCache::<1>::default();

        let _ = cache.get_or_insert_with(key(1, 2), || PairPositioning::Kerning(px(1)));
        let _ = cache.get_or_insert_with(key(1, 2), || PairPositioning::Kerning(px(2)));
        let _ = cache.get_or_insert_with(key(3, 4), || PairPositioning::Kerning(px(3)));

        let metrics = cache.metrics();

        assert_eq!(metrics.lookups, 3);
        assert_eq!(metrics.hits, 1);
        assert_eq!(metrics.misses, 2);
        assert_eq!(metrics.collisions, 1);
    }

    #[test]
    fn linear_probe_retains_colliding_pairs() {
        let mut cache = PairPositioningCache::<2>::default();
        let computes = Cell::new(0u32);

        // With the current hash these two keys have the same home slot for a two-entry cache.
        let first_key = key(1, 2);
        let second_key = key(3, 4);

        let first = cache.get_or_insert_with(first_key, || {
            computes.set(computes.get() + 1);
            PairPositioning::Kerning(px(3))
        });

        let second = cache.get_or_insert_with(second_key, || {
            computes.set(computes.get() + 1);
            PairPositioning::Kerning(px(7))
        });

        let first_again = cache.get_or_insert_with(first_key, || {
            computes.set(computes.get() + 1);
            PairPositioning::Kerning(px(99))
        });

        let second_again = cache.get_or_insert_with(second_key, || {
            computes.set(computes.get() + 1);
            PairPositioning::Kerning(px(99))
        });

        assert_eq!(first, PairPositioning::Kerning(px(3)));
        assert_eq!(second, PairPositioning::Kerning(px(7)));
        assert_eq!(first_again, PairPositioning::Kerning(px(3)));
        assert_eq!(second_again, PairPositioning::Kerning(px(7)));

        assert_eq!(computes.get(), 2);
    }
}
