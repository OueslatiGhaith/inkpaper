use alloc::{string::String, vec::Vec};
use inkpaper_epub::{BookLocation, ContentOffset, SpineIndex};
use inkpaper_reader::ReadingPosition;
use minicbor::{Decode, Encode};

mod progress;
mod state;

pub use progress::BookProgress;
pub(crate) use state::{ReadingHistoryRequest, ReadingHistoryState};

use crate::storage::{self, StorageError};

pub const MAX_READING_HISTORY_ENTRIES: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadingHistoryEntry {
    path: String,
    identifier: Option<String>,
    title: String,
    creator: Option<String>,
    position: ReadingPosition,
    progress: BookProgress,
}

impl ReadingHistoryEntry {
    pub fn new(
        path: String,
        identifier: Option<String>,
        title: String,
        creator: Option<String>,
        position: ReadingPosition,
        progress: BookProgress,
    ) -> Self {
        Self {
            path,
            identifier,
            title,
            creator: creator.filter(|creator| !creator.trim().is_empty()),
            position,
            progress,
        }
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn identifier(&self) -> Option<&str> {
        self.identifier.as_deref()
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn creator(&self) -> Option<&str> {
        self.creator.as_deref()
    }

    pub const fn position(&self) -> ReadingPosition {
        self.position
    }

    pub const fn progress(&self) -> BookProgress {
        self.progress
    }

    pub fn display_title(&self) -> &str {
        self.title()
    }

    pub fn display_subtitle(&self) -> &str {
        self.creator()
            .filter(|creator| !creator.trim().is_empty())
            .unwrap_or_else(|| parent_path(&self.path))
    }
}

fn parent_path(path: &str) -> &str {
    match path.rfind('/') {
        Some(0) => "/",
        Some(index) => &path[..index],
        None => "",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadingHistoryError {
    Storage(StorageError),
    TooManyEntries,
}

impl From<StorageError> for ReadingHistoryError {
    fn from(error: StorageError) -> Self {
        Self::Storage(error)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReadingHistory {
    entries: Vec<ReadingHistoryEntry>,
}

impl ReadingHistory {
    pub fn entries(&self) -> &[ReadingHistoryEntry] {
        &self.entries
    }

    pub fn record(&mut self, entry: ReadingHistoryEntry) {
        if let Some(index) = self
            .entries
            .iter()
            .position(|existing| existing.path() == entry.path())
        {
            self.entries.remove(index);
        }

        self.entries.insert(0, entry);

        self.entries.truncate(MAX_READING_HISTORY_ENTRIES);
    }

    pub fn find(&self, path: &str) -> Option<&ReadingHistoryEntry> {
        self.entries.iter().find(|entry| entry.path() == path)
    }

    pub fn resume_position(&self, path: &str, identifier: Option<&str>) -> Option<ReadingPosition> {
        let entry = self.find(path)?;

        if entry.identifier() != identifier {
            return None;
        }

        Some(entry.position())
    }

    pub fn encode(&self) -> Result<Vec<u8>, ReadingHistoryError> {
        if self.entries.len() > MAX_READING_HISTORY_ENTRIES {
            return Err(ReadingHistoryError::TooManyEntries);
        }

        let encoded = EncodedHistory {
            entries: EncodedHistoryEntries(&self.entries),
        };

        Ok(storage::encode(&encoded)?)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, ReadingHistoryError> {
        let stored: StoredHistory = storage::decode(bytes)?;

        if stored.entries.len() > MAX_READING_HISTORY_ENTRIES {
            return Err(ReadingHistoryError::TooManyEntries);
        }

        Ok(Self {
            entries: stored
                .entries
                .into_iter()
                .map(ReadingHistoryEntry::from)
                .collect(),
        })
    }
}

#[derive(Encode)]
struct EncodedHistory<'a> {
    #[n(0)]
    entries: EncodedHistoryEntries<'a>,
}

struct EncodedHistoryEntries<'a>(&'a [ReadingHistoryEntry]);

impl<C> Encode<C> for EncodedHistoryEntries<'_> {
    fn encode<W: minicbor::encode::Write>(
        &self,
        e: &mut minicbor::Encoder<W>,
        ctx: &mut C,
    ) -> Result<(), minicbor::encode::Error<W::Error>> {
        e.array(self.0.len() as u64)?;
        for entry in self.0 {
            EncodedHistoryEntry::from(entry).encode(e, ctx)?;
        }
        Ok(())
    }
}

#[derive(Encode)]
struct EncodedHistoryEntry<'a> {
    #[n(0)]
    path: &'a str,
    #[n(1)]
    identifier: Option<&'a str>,
    #[n(2)]
    title: &'a str,
    #[n(3)]
    creator: Option<&'a str>,
    #[n(4)]
    position: StoredPosition,
    #[n(5)]
    progress_basis_points: u16,
}

impl<'a> From<&'a ReadingHistoryEntry> for EncodedHistoryEntry<'a> {
    fn from(entry: &'a ReadingHistoryEntry) -> Self {
        let position = entry.position();
        let location = position.location();

        Self {
            path: &entry.path,
            identifier: entry.identifier.as_deref(),
            title: &entry.title,
            creator: entry.creator.as_deref(),
            position: StoredPosition {
                spine: location.spine().get(),
                offset: location.offset().get(),
                non_text: position.non_text(),
            },
            progress_basis_points: entry.progress().basis_points(),
        }
    }
}

#[derive(Debug, Decode)]
struct StoredHistory {
    #[n(0)]
    entries: Vec<StoredHistoryEntry>,
}

#[derive(Debug, Decode)]
struct StoredHistoryEntry {
    #[n(0)]
    path: String,
    #[n(1)]
    identifier: Option<String>,
    #[n(2)]
    title: String,
    #[n(3)]
    creator: Option<String>,
    #[n(4)]
    position: StoredPosition,
    #[n(5)]
    progress_basis_points: u16,
}

#[derive(Debug, Encode, Decode)]
struct StoredPosition {
    #[n(0)]
    spine: u32,
    #[n(1)]
    offset: u64,
    #[n(2)]
    non_text: u64,
}

impl From<StoredHistoryEntry> for ReadingHistoryEntry {
    fn from(entry: StoredHistoryEntry) -> Self {
        Self::new(
            entry.path,
            entry.identifier,
            entry.title,
            entry.creator,
            ReadingPosition::new(
                BookLocation::new(
                    SpineIndex::new(entry.position.spine),
                    ContentOffset::new(entry.position.offset),
                ),
                entry.position.non_text,
            ),
            BookProgress::from_basis_points(entry.progress_basis_points),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn position(spine: u32, offset: u64) -> ReadingPosition {
        ReadingPosition::new(
            BookLocation::new(SpineIndex::new(spine), ContentOffset::new(offset)),
            0,
        )
    }

    #[test]
    fn recording_existing_book_refreshes_metadata_and_recency() {
        let mut history = ReadingHistory::default();

        history.record(ReadingHistoryEntry::new(
            String::from("/a.epub"),
            None,
            String::from("Old title"),
            None,
            position(0, 1),
            BookProgress::ZERO,
        ));

        history.record(ReadingHistoryEntry::new(
            String::from("/b.epub"),
            None,
            String::from("Book B"),
            None,
            position(0, 2),
            BookProgress::ZERO,
        ));

        history.record(ReadingHistoryEntry::new(
            String::from("/a.epub"),
            None,
            String::from("New title"),
            Some(String::from("Author A")),
            position(3, 42),
            BookProgress::ZERO,
        ));

        assert_eq!(history.entries().len(), 2);

        assert_eq!(history.entries()[0].path(), "/a.epub");

        assert_eq!(history.entries()[0].title(), "New title");

        assert_eq!(history.entries()[0].creator(), Some("Author A"));

        assert_eq!(history.entries()[1].path(), "/b.epub");
    }

    #[test]
    fn resume_rejects_different_book_identifier() {
        let mut history = ReadingHistory::default();

        history.record(ReadingHistoryEntry::new(
            String::from("/book.epub"),
            Some(String::from("book-a")),
            String::from("Book"),
            None,
            ReadingPosition::default(),
            BookProgress::ZERO,
        ));

        assert!(
            history
                .resume_position("/book.epub", Some("book-b"))
                .is_none()
        );

        assert_eq!(
            history.resume_position("/book.epub", Some("book-a")),
            Some(ReadingPosition::default()),
        );
    }

    #[test]
    fn recent_book_prefers_creator_as_subtitle() {
        let entry = ReadingHistoryEntry::new(
            String::from("/Books/Dune.epub"),
            None,
            String::from("Dune"),
            Some(String::from("Frank Herbert")),
            ReadingPosition::default(),
            BookProgress::ZERO,
        );

        assert_eq!(entry.display_title(), "Dune");

        assert_eq!(entry.display_subtitle(), "Frank Herbert");
    }

    #[test]
    fn recent_book_falls_back_to_parent_path_without_creator() {
        let entry = ReadingHistoryEntry::new(
            String::from("/Books/Dune.epub"),
            None,
            String::from("Dune"),
            None,
            ReadingPosition::default(),
            BookProgress::ZERO,
        );

        assert_eq!(entry.display_subtitle(), "/Books");
    }

    #[test]
    fn decoder_rejects_trailing_data() {
        let history = ReadingHistory::default();
        let mut encoded = history.encode().unwrap();

        encoded.push(0xff);

        assert_eq!(
            ReadingHistory::decode(&encoded),
            Err(ReadingHistoryError::Storage(StorageError::TrailingData)),
        );
    }

    #[test]
    fn history_round_trips_metadata_position_and_progress() {
        let entry = ReadingHistoryEntry::new(
            String::from("/Books/Étranger.epub"),
            Some(String::from("urn:isbn:123")),
            String::from("L'Étranger"),
            Some(String::from("Albert Camus")),
            ReadingPosition::new(
                BookLocation::new(SpineIndex::new(7), ContentOffset::new(1234)),
                9,
            ),
            BookProgress::from_basis_points(4_321),
        );

        let mut history = ReadingHistory::default();

        history.record(entry.clone());

        let encoded = history.encode().unwrap();

        let decoded = ReadingHistory::decode(&encoded).unwrap();

        assert_eq!(decoded.entries(), &[entry]);
    }
}
