use inkpaper_epub::BookLocation;

/// a position in one book's normalized chapter flow
/// - `location` counts text as EPUB Unicode scalar offsets.
/// - `non_text` counts all preceeding images and explicit breaks in the chapter, including
///   hiddren or unrenderable ones.
///
/// Neither coordinates depend on fonts or page geometry.
///
/// Positions belong to the same book revision and normalization rules. A platform must
/// associate persisted positions with that book, not just a page number
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReadingPosition {
    location: BookLocation,
    non_text: u64,
}

impl ReadingPosition {
    pub const fn new(location: BookLocation, non_text: u64) -> Self {
        Self { location, non_text }
    }

    pub const fn location(self) -> BookLocation {
        self.location
    }

    pub const fn non_text(self) -> u64 {
        self.non_text
    }

    pub(crate) fn advance_text(self, text: &str) -> Self {
        Self::new(
            BookLocation::new(
                self.location.spine(),
                self.location.offset().advance_text(text),
            ),
            self.non_text,
        )
    }

    pub(crate) fn advance_non_text(self) -> Self {
        Self::new(self.location, self.non_text.saturating_add(1))
    }
}
