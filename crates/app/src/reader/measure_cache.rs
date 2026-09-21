use inkpaper_epub::{BlockKind, FontStyle, FontWeight};
use inkpaper_reader::TextStyle;

pub(super) const READER_MEASURE_CACHE_SLOTS: usize = 64;
pub(super) const READER_MEASURE_CACHE_TEXT_BYTES: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ReaderMeasureCacheLookup {
    Hit(u32),

    Miss { slot: usize, collision: bool },

    Bypass,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ReaderMeasureCacheEntry<const TEXT_BYTES: usize> {
    style: TextStyle,
    len: u8,
    text: [u8; TEXT_BYTES],
    width: u32,
}

impl<const TEXT_BYTES: usize> ReaderMeasureCacheEntry<TEXT_BYTES> {
    fn matches(&self, text: &str, style: TextStyle) -> bool {
        let len = usize::from(self.len);

        self.style == style && len == text.len() && self.text[..len] == *text.as_bytes()
    }
}

pub(super) struct ReaderMeasureCache<const SLOTS: usize, const TEXT_BYTES: usize> {
    slots: [Option<ReaderMeasureCacheEntry<TEXT_BYTES>>; SLOTS],
}

impl<const SLOTS: usize, const TEXT_BYTES: usize> Default
    for ReaderMeasureCache<SLOTS, TEXT_BYTES>
{
    fn default() -> Self {
        assert!(
            SLOTS > 0,
            "reader measurement cache must contain at least one slot",
        );

        assert!(
            TEXT_BYTES <= usize::from(u8::MAX),
            "reader measurement cache token capacity must fit in u8",
        );

        Self {
            slots: [None; SLOTS],
        }
    }
}

impl<const SLOTS: usize, const TEXT_BYTES: usize> ReaderMeasureCache<SLOTS, TEXT_BYTES> {
    pub(super) fn lookup(&self, text: &str, style: TextStyle) -> ReaderMeasureCacheLookup {
        if text.len() > TEXT_BYTES {
            return ReaderMeasureCacheLookup::Bypass;
        }

        let home_slot = Self::slot_index(text, style);

        for probe in 0..SLOTS {
            let slot = (home_slot + probe) % SLOTS;

            match self.slots[slot] {
                Some(entry) if entry.matches(text, style) => {
                    return ReaderMeasureCacheLookup::Hit(entry.width);
                }

                Some(_) => {}

                None => {
                    return ReaderMeasureCacheLookup::Miss {
                        slot,
                        collision: probe != 0,
                    };
                }
            }
        }

        // Once the table is full there is no empty terminator.
        // Replace the home slot on the next successful measurement.
        ReaderMeasureCacheLookup::Miss {
            slot: home_slot,
            collision: true,
        }
    }

    pub(super) fn insert(&mut self, slot: usize, text: &str, style: TextStyle, width: u32) {
        debug_assert!(slot < SLOTS);
        debug_assert!(text.len() <= TEXT_BYTES);

        let len = u8::try_from(text.len())
            .expect("cacheable reader measurement token length must fit in u8");

        let mut bytes = [0u8; TEXT_BYTES];

        bytes[..text.len()].copy_from_slice(text.as_bytes());

        self.slots[slot] = Some(ReaderMeasureCacheEntry {
            style,
            len,
            text: bytes,
            width,
        });
    }

    fn slot_index(text: &str, style: TextStyle) -> usize {
        let mut hash = 0x811C_9DC5u32;

        for byte in text.bytes() {
            hash = hash_byte(hash, byte);
        }

        for byte in style.font_size().to_le_bytes() {
            hash = hash_byte(hash, byte);
        }

        let (block_kind, heading_level) = match style.block_kind() {
            BlockKind::Paragraph => (0u8, 0u8),
            BlockKind::Heading(level) => (1u8, level),
            BlockKind::ListItem => (2u8, 0u8),
        };

        hash = hash_byte(hash, block_kind);
        hash = hash_byte(hash, heading_level);

        hash = hash_byte(
            hash,
            match style.font_weight() {
                FontWeight::Normal => 0,
                FontWeight::Bold => 1,
            },
        );

        hash = hash_byte(
            hash,
            match style.font_style() {
                FontStyle::Normal => 0,
                FontStyle::Italic => 1,
            },
        );

        usize::try_from(hash).unwrap_or(usize::MAX) % SLOTS
    }
}

fn hash_byte(hash: u32, byte: u8) -> u32 {
    (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style(font_size: u16) -> TextStyle {
        TextStyle::new(
            font_size,
            BlockKind::Paragraph,
            FontWeight::Normal,
            FontStyle::Normal,
        )
    }

    #[test]
    fn repeated_exact_token_hits() {
        let mut cache = ReaderMeasureCache::<8, 16>::default();
        let style = style(20);

        let miss = cache.lookup("hello", style);

        let ReaderMeasureCacheLookup::Miss { slot, collision } = miss else {
            panic!("first lookup must miss");
        };

        assert!(!collision);

        cache.insert(slot, "hello", style, 42);

        assert_eq!(
            cache.lookup("hello", style),
            ReaderMeasureCacheLookup::Hit(42),
        );
    }

    #[test]
    fn same_text_with_different_style_does_not_hit() {
        let mut cache = ReaderMeasureCache::<8, 16>::default();

        let first_style = style(20);
        let second_style = style(22);

        let ReaderMeasureCacheLookup::Miss { slot, .. } = cache.lookup("hello", first_style) else {
            panic!("first lookup must miss");
        };

        cache.insert(slot, "hello", first_style, 42);

        assert_eq!(
            cache.lookup("hello", first_style),
            ReaderMeasureCacheLookup::Hit(42),
        );

        assert!(matches!(
            cache.lookup("hello", second_style),
            ReaderMeasureCacheLookup::Miss { .. },
        ));
    }

    #[test]
    fn collision_never_returns_another_tokens_width() {
        let mut cache = ReaderMeasureCache::<1, 16>::default();
        let style = style(20);

        let ReaderMeasureCacheLookup::Miss { slot, .. } = cache.lookup("first", style) else {
            panic!("first lookup must miss");
        };

        cache.insert(slot, "first", style, 11);

        let ReaderMeasureCacheLookup::Miss { slot, collision } = cache.lookup("second", style)
        else {
            panic!("colliding text must miss");
        };

        assert!(collision);

        cache.insert(slot, "second", style, 22);

        assert_eq!(
            cache.lookup("second", style),
            ReaderMeasureCacheLookup::Hit(22),
        );

        assert!(matches!(
            cache.lookup("first", style),
            ReaderMeasureCacheLookup::Miss {
                collision: true,
                ..
            },
        ));
    }

    #[test]
    fn token_larger_than_inline_capacity_bypasses_cache() {
        let cache = ReaderMeasureCache::<8, 4>::default();

        assert_eq!(
            cache.lookup("hello", style(20)),
            ReaderMeasureCacheLookup::Bypass,
        );
    }

    #[test]
    fn utf8_token_is_compared_by_exact_bytes() {
        let mut cache = ReaderMeasureCache::<8, 16>::default();
        let style = style(20);

        let ReaderMeasureCacheLookup::Miss { slot, .. } = cache.lookup("café", style) else {
            panic!("first lookup must miss");
        };

        cache.insert(slot, "café", style, 37);

        assert_eq!(
            cache.lookup("café", style),
            ReaderMeasureCacheLookup::Hit(37),
        );

        assert!(matches!(
            cache.lookup("cafe", style),
            ReaderMeasureCacheLookup::Miss { .. },
        ));
    }
}
