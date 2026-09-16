use alloc::{string::String, vec::Vec};

use inkpaper_epub::{BookLocation, ContentOffset, SpineIndex};

use inkpaper_reader::ReadingPosition;

const MAGIC: [u8; 8] = *b"INKHST02";
const NONE_LEN: u16 = u16::MAX;

pub const MAX_READING_HISTORY_ENTRIES: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadingHistoryEntry {
    path: String,
    identifier: Option<String>,
    title: String,
    creator: Option<String>,
    position: ReadingPosition,
}

impl ReadingHistoryEntry {
    pub fn new(
        path: String,
        identifier: Option<String>,
        title: String,
        creator: Option<String>,
        position: ReadingPosition,
    ) -> Self {
        Self {
            path,
            identifier,
            title,
            creator: creator.filter(|creator| !creator.trim().is_empty()),
            position,
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
    InvalidMagic,
    InvalidUtf8,
    TooManyEntries,
    FieldTooLong,
    Truncated,
    TrailingData,
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

        let count =
            u16::try_from(self.entries.len()).map_err(|_| ReadingHistoryError::TooManyEntries)?;

        let mut output = Vec::new();

        output.extend_from_slice(&MAGIC);
        output.extend_from_slice(&count.to_le_bytes());

        for entry in &self.entries {
            let path_len = required_len(entry.path())?;

            let identifier_len = optional_len(entry.identifier())?;

            let title_len = required_len(entry.title())?;

            let creator_len = optional_len(entry.creator())?;

            let location = entry.position().location();

            output.extend_from_slice(&path_len.to_le_bytes());
            output.extend_from_slice(&identifier_len.to_le_bytes());
            output.extend_from_slice(&title_len.to_le_bytes());
            output.extend_from_slice(&creator_len.to_le_bytes());
            output.extend_from_slice(&location.spine().get().to_le_bytes());
            output.extend_from_slice(&location.offset().get().to_le_bytes());
            output.extend_from_slice(&entry.position().non_text().to_le_bytes());
            output.extend_from_slice(entry.path().as_bytes());

            if let Some(identifier) = entry.identifier() {
                output.extend_from_slice(identifier.as_bytes());
            }

            output.extend_from_slice(entry.title().as_bytes());

            if let Some(creator) = entry.creator() {
                output.extend_from_slice(creator.as_bytes());
            }
        }

        Ok(output)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, ReadingHistoryError> {
        let mut cursor = 0usize;

        if read_array::<8>(bytes, &mut cursor)? != MAGIC {
            return Err(ReadingHistoryError::InvalidMagic);
        }

        let count = usize::from(u16::from_le_bytes(read_array::<2>(bytes, &mut cursor)?));
        if count > MAX_READING_HISTORY_ENTRIES {
            return Err(ReadingHistoryError::TooManyEntries);
        }

        let mut entries = Vec::with_capacity(count);

        for _ in 0..count {
            let path_len = usize::from(u16::from_le_bytes(read_array::<2>(bytes, &mut cursor)?));

            let identifier_len = u16::from_le_bytes(read_array::<2>(bytes, &mut cursor)?);
            let title_len = usize::from(u16::from_le_bytes(read_array::<2>(bytes, &mut cursor)?));
            let creator_len = u16::from_le_bytes(read_array::<2>(bytes, &mut cursor)?);

            let spine = u32::from_le_bytes(read_array::<4>(bytes, &mut cursor)?);

            let offset = u64::from_le_bytes(read_array::<8>(bytes, &mut cursor)?);

            let non_text = u64::from_le_bytes(read_array::<8>(bytes, &mut cursor)?);

            let path = read_string(bytes, &mut cursor, path_len)?;

            let identifier = read_optional_string(bytes, &mut cursor, identifier_len)?;

            let title = read_string(bytes, &mut cursor, title_len)?;

            let creator = read_optional_string(bytes, &mut cursor, creator_len)?;

            entries.push(ReadingHistoryEntry::new(
                path,
                identifier,
                title,
                creator,
                ReadingPosition::new(
                    BookLocation::new(SpineIndex::new(spine), ContentOffset::new(offset)),
                    non_text,
                ),
            ));
        }

        if cursor != bytes.len() {
            return Err(ReadingHistoryError::TrailingData);
        }

        Ok(Self { entries })
    }
}

fn required_len(value: &str) -> Result<u16, ReadingHistoryError> {
    u16::try_from(value.len()).map_err(|_| ReadingHistoryError::FieldTooLong)
}

fn optional_len(value: Option<&str>) -> Result<u16, ReadingHistoryError> {
    let Some(value) = value else {
        return Ok(NONE_LEN);
    };

    let len = u16::try_from(value.len()).map_err(|_| ReadingHistoryError::FieldTooLong)?;

    if len == NONE_LEN {
        return Err(ReadingHistoryError::FieldTooLong);
    }

    Ok(len)
}

fn read_string(
    bytes: &[u8],
    cursor: &mut usize,
    len: usize,
) -> Result<String, ReadingHistoryError> {
    let bytes = read_slice(bytes, cursor, len)?;

    let value = core::str::from_utf8(bytes).map_err(|_| ReadingHistoryError::InvalidUtf8)?;

    Ok(String::from(value))
}

fn read_optional_string(
    bytes: &[u8],
    cursor: &mut usize,
    len: u16,
) -> Result<Option<String>, ReadingHistoryError> {
    if len == NONE_LEN {
        return Ok(None);
    }

    read_string(bytes, cursor, usize::from(len)).map(Some)
}

fn read_array<const N: usize>(
    bytes: &[u8],
    cursor: &mut usize,
) -> Result<[u8; N], ReadingHistoryError> {
    let end = cursor
        .checked_add(N)
        .ok_or(ReadingHistoryError::Truncated)?;

    let source = bytes
        .get(*cursor..end)
        .ok_or(ReadingHistoryError::Truncated)?;

    let mut output = [0u8; N];
    output.copy_from_slice(source);

    *cursor = end;

    Ok(output)
}

fn read_slice<'a>(
    bytes: &'a [u8],
    cursor: &mut usize,
    len: usize,
) -> Result<&'a [u8], ReadingHistoryError> {
    let end = cursor
        .checked_add(len)
        .ok_or(ReadingHistoryError::Truncated)?;

    let output = bytes
        .get(*cursor..end)
        .ok_or(ReadingHistoryError::Truncated)?;

    *cursor = end;

    Ok(output)
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
    fn history_round_trips_metadata_and_position() {
        let entry = ReadingHistoryEntry::new(
            String::from("/Books/Étranger.epub"),
            Some(String::from("urn:isbn:123")),
            String::from("L'Étranger"),
            Some(String::from("Albert Camus")),
            ReadingPosition::new(
                BookLocation::new(SpineIndex::new(7), ContentOffset::new(1234)),
                9,
            ),
        );

        let mut history = ReadingHistory::default();

        history.record(entry.clone());

        let encoded = history.encode().unwrap();

        let decoded = ReadingHistory::decode(&encoded).unwrap();

        assert_eq!(decoded.entries(), &[entry]);
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
        ));

        history.record(ReadingHistoryEntry::new(
            String::from("/b.epub"),
            None,
            String::from("Book B"),
            None,
            position(0, 2),
        ));

        history.record(ReadingHistoryEntry::new(
            String::from("/a.epub"),
            None,
            String::from("New title"),
            Some(String::from("Author A")),
            position(3, 42),
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
        );

        assert_eq!(entry.display_subtitle(), "/Books");
    }
}
