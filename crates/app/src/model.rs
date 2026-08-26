use core::fmt::Write;

use heapless::{String, Vec};

use crate::{
    clock::{Clock, UtcOffset},
    storage::layout,
};

pub const BOOK_PATH_BYTES: usize = 256;
pub const LIBRARY_PAGE_CAPACITY: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ModelError {
    TextTooLong,
    PathTooLong,
    InvalidBookPath,
    LibraryPageFull,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Percent {
    value: u8,
    label: String<4>,
}

impl Percent {
    pub fn new(value: u8) -> Self {
        let value = value.min(100);
        let mut label = String::new();

        write!(&mut label, "{value}%").expect("a clamped percentage must fit in four bytes");

        Self { value, label }
    }

    pub const fn value(&self) -> u8 {
        self.value
    }

    pub fn label(&self) -> &str {
        self.label.as_str()
    }

    pub fn set(&mut self, value: u8) {
        *self = Self::new(value);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct BookSummary {
    title: String<96>,
    author: String<64>,
    progress: Percent,
}

impl BookSummary {
    pub fn try_new(title: &str, author: &str, progress_percent: u8) -> Result<Self, ModelError> {
        let mut stored_title = String::new();
        stored_title
            .push_str(title)
            .map_err(|_| ModelError::TextTooLong)?;

        let mut stored_author = String::new();
        stored_author
            .push_str(author)
            .map_err(|_| ModelError::TextTooLong)?;

        Ok(Self {
            title: stored_title,
            author: stored_author,
            progress: Percent::new(progress_percent),
        })
    }

    pub fn title(&self) -> &str {
        self.title.as_str()
    }

    pub fn author(&self) -> &str {
        self.author.as_str()
    }

    pub const fn progress(&self) -> &Percent {
        &self.progress
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct LibraryEntry {
    path: String<BOOK_PATH_BYTES>,
    summary: BookSummary,
}

impl LibraryEntry {
    pub fn try_new(path: &str, summary: BookSummary) -> Result<Self, ModelError> {
        if !layout::is_book_path(path) {
            return Err(ModelError::InvalidBookPath);
        }

        let mut stored_path = String::new();

        stored_path
            .push_str(path)
            .map_err(|_| ModelError::PathTooLong)?;

        Ok(Self {
            path: stored_path,

            summary,
        })
    }

    pub fn path(&self) -> &str {
        self.path.as_str()
    }

    pub const fn summary(&self) -> &BookSummary {
        &self.summary
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct LibraryPage {
    entries: Vec<LibraryEntry, LIBRARY_PAGE_CAPACITY>,
}

impl Default for LibraryPage {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
}

impl LibraryPage {
    pub fn entries(&self) -> &[LibraryEntry] {
        self.entries.as_slice()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn try_push(&mut self, entry: LibraryEntry) -> Result<(), ModelError> {
        self.entries
            .push(entry)
            .map_err(|_| ModelError::LibraryPageFull)
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct AppModel {
    battery: Percent,
    clock: Clock,
    current_book: BookSummary,
    library: LibraryPage,
}

impl AppModel {
    pub fn new(battery_percent: u8, current_book: BookSummary) -> Self {
        Self {
            battery: Percent::new(battery_percent),
            clock: Clock::new(UtcOffset::UTC),
            current_book,
            library: LibraryPage::default(),
        }
    }

    pub const fn battery(&self) -> &Percent {
        &self.battery
    }

    pub const fn clock(&self) -> &Clock {
        &self.clock
    }

    pub fn clock_mut(&mut self) -> &mut Clock {
        &mut self.clock
    }

    pub const fn current_book(&self) -> &BookSummary {
        &self.current_book
    }

    pub const fn library(&self) -> &LibraryPage {
        &self.library
    }

    pub fn library_mut(&mut self) -> &mut LibraryPage {
        &mut self.library
    }

    pub fn set_battery_percent(&mut self, percent: u8) {
        self.battery.set(percent);
    }

    pub fn set_current_book(&mut self, book: BookSummary) {
        self.current_book = book;
    }
}
