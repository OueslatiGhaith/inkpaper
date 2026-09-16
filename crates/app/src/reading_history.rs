use alloc::{string::String, vec::Vec};

use inkpaper_epub::{BookLocation, ContentOffset, SpineIndex};
use inkpaper_reader::ReadingPosition;

const MAGIC: [u8; 8] = *b"INKPRD01";
const NO_IDENTIFIER: u16 = u16::MAX;

pub const MAX_READING_HISTORY_ENTRIES: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadingProgress {
    path: String,
    identifier: Option<String>,
    position: ReadingPosition,
}

impl ReadingProgress {
    pub fn new(path: String, identifier: Option<String>, position: ReadingPosition) -> Self {
        Self {
            path,
            identifier,
            position,
        }
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn identifier(&self) -> Option<&str> {
        self.identifier.as_deref()
    }

    pub const fn position(&self) -> ReadingPosition {
        self.position
    }

    pub fn display_title(&self) -> &str {
        epub_title_from_path(&self.path)
    }

    pub fn display_subtitle(&self) -> &str {
        parent_path(&self.path)
    }
}

fn epub_title_from_path(path: &str) -> &str {
    let name = path
        .rsplit('/')
        .find(|segment| !segment.is_empty())
        .unwrap_or(path);

    let Some(suffix_start) = name.len().checked_sub(5) else {
        return name;
    };

    let is_epub = name
        .get(suffix_start..)
        .is_some_and(|suffix| suffix.eq_ignore_ascii_case(".epub"));

    if is_epub { &name[..suffix_start] } else { name }
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
    entries: Vec<ReadingProgress>,
}

impl ReadingHistory {
    pub fn entries(&self) -> &[ReadingProgress] {
        &self.entries
    }

    pub fn record(&mut self, progress: ReadingProgress) {
        if let Some(index) = self
            .entries
            .iter()
            .position(|entry| entry.path() == progress.path())
        {
            self.entries.remove(index);
        }

        self.entries.insert(0, progress);

        self.entries.truncate(MAX_READING_HISTORY_ENTRIES);
    }

    pub fn find(&self, path: &str) -> Option<&ReadingProgress> {
        self.entries.iter().find(|entry| entry.path() == path)
    }

    pub fn resume_position(&self, path: &str, identifier: Option<&str>) -> Option<ReadingPosition> {
        let progress = self.find(path)?;

        if progress.identifier() != identifier {
            return None;
        }

        Some(progress.position())
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
            let path = entry.path.as_bytes();

            let path_len =
                u16::try_from(path.len()).map_err(|_| ReadingHistoryError::FieldTooLong)?;

            let identifier_len = match &entry.identifier {
                Some(identifier) => {
                    let len = identifier.len();

                    if len >= usize::from(NO_IDENTIFIER) {
                        return Err(ReadingHistoryError::FieldTooLong);
                    }

                    u16::try_from(len).map_err(|_| ReadingHistoryError::FieldTooLong)?
                }

                None => NO_IDENTIFIER,
            };

            let location = entry.position.location();

            output.extend_from_slice(&path_len.to_le_bytes());
            output.extend_from_slice(&identifier_len.to_le_bytes());
            output.extend_from_slice(&location.spine().get().to_le_bytes());
            output.extend_from_slice(&location.offset().get().to_le_bytes());
            output.extend_from_slice(&entry.position.non_text().to_le_bytes());
            output.extend_from_slice(path);

            if let Some(identifier) = &entry.identifier {
                output.extend_from_slice(identifier.as_bytes());
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

            let spine = u32::from_le_bytes(read_array::<4>(bytes, &mut cursor)?);

            let offset = u64::from_le_bytes(read_array::<8>(bytes, &mut cursor)?);

            let non_text = u64::from_le_bytes(read_array::<8>(bytes, &mut cursor)?);

            let path_bytes = read_slice(bytes, &mut cursor, path_len)?;

            let path =
                core::str::from_utf8(path_bytes).map_err(|_| ReadingHistoryError::InvalidUtf8)?;

            let identifier = if identifier_len == NO_IDENTIFIER {
                None
            } else {
                let identifier_bytes = read_slice(bytes, &mut cursor, usize::from(identifier_len))?;

                let identifier = core::str::from_utf8(identifier_bytes)
                    .map_err(|_| ReadingHistoryError::InvalidUtf8)?;

                Some(String::from(identifier))
            };

            entries.push(ReadingProgress::new(
                String::from(path),
                identifier,
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

    #[test]
    fn reading_history_round_trips_stable_positions() {
        let progress = ReadingProgress::new(
            String::from("/Books/Étranger.epub"),
            Some(String::from("urn:isbn:123")),
            ReadingPosition::new(
                BookLocation::new(SpineIndex::new(7), ContentOffset::new(1234)),
                9,
            ),
        );

        let mut history = ReadingHistory::default();

        history.record(progress.clone());

        let encoded = history.encode().unwrap();

        let decoded = ReadingHistory::decode(&encoded).unwrap();

        assert_eq!(decoded.entries(), &[progress]);
    }

    #[test]
    fn recording_existing_book_moves_it_to_front() {
        let position = ReadingPosition::default();

        let mut history = ReadingHistory::default();

        history.record(ReadingProgress::new(
            String::from("/a.epub"),
            None,
            position,
        ));

        history.record(ReadingProgress::new(
            String::from("/b.epub"),
            None,
            position,
        ));

        history.record(ReadingProgress::new(
            String::from("/a.epub"),
            None,
            ReadingPosition::new(
                BookLocation::new(SpineIndex::new(3), ContentOffset::new(42)),
                1,
            ),
        ));

        assert_eq!(history.entries()[0].path(), "/a.epub");
        assert_eq!(history.entries()[1].path(), "/b.epub");
        assert_eq!(history.entries().len(), 2);
    }

    #[test]
    fn resume_rejects_different_book_identifier() {
        let mut history = ReadingHistory::default();

        history.record(ReadingProgress::new(
            String::from("/book.epub"),
            Some(String::from("book-a")),
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
    fn recent_book_display_name_comes_from_real_path() {
        let progress = ReadingProgress::new(
            String::from("/Books/Sci-Fi/The Left Hand of Darkness.EPUB"),
            None,
            ReadingPosition::default(),
        );

        assert_eq!(progress.display_title(), "The Left Hand of Darkness");
        assert_eq!(progress.display_subtitle(), "/Books/Sci-Fi");
    }

    #[test]
    fn root_level_recent_book_uses_root_as_subtitle() {
        let progress =
            ReadingProgress::new(String::from("/Dune.epub"), None, ReadingPosition::default());

        assert_eq!(progress.display_title(), "Dune");
        assert_eq!(progress.display_subtitle(), "/");
    }
}
