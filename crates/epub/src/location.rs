#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SpineIndex(u32);

impl SpineIndex {
    pub const ZERO: Self = Self(0);

    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u32 {
        self.0
    }

    pub fn try_from_usize(value: usize) -> Option<Self> {
        u32::try_from(value).ok().map(Self)
    }

    pub fn as_usize(self) -> Option<usize> {
        usize::try_from(self.0).ok()
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContentOffset(u64);

impl ContentOffset {
    pub const ZERO: Self = Self(0);

    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    pub fn advance_text(self, text: &str) -> Self {
        let length = text
            .chars()
            .fold(0u64, |length, _| length.saturating_add(1));

        Self(self.0.saturating_add(length))
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BookLocation {
    spine: SpineIndex,
    offset: ContentOffset,
}

impl BookLocation {
    pub const fn new(spine: SpineIndex, offset: ContentOffset) -> Self {
        Self { spine, offset }
    }

    pub const fn spine(self) -> SpineIndex {
        self.spine
    }

    pub const fn offset(self) -> ContentOffset {
        self.offset
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_offset_counts_unicode_scalars_not_utf8_bytes() {
        let offset = ContentOffset::new(7).advance_text("é🙂漢");

        assert_eq!(offset, ContentOffset::new(10),);
    }

    #[test]
    fn book_location_keeps_spine_and_content_position_separate() {
        let location = BookLocation::new(SpineIndex::new(12), ContentOffset::new(3456));

        assert_eq!(location.spine(), SpineIndex::new(12),);
        assert_eq!(location.offset(), ContentOffset::new(3456),);
    }

    #[test]
    fn spine_index_converts_to_and_from_usize() {
        let index = SpineIndex::try_from_usize(42).unwrap();

        assert_eq!(index, SpineIndex::new(42),);
        assert_eq!(index.as_usize(), Some(42),);
    }
}
