#![no_std]

extern crate alloc;

mod app;
mod browser;
mod components;
mod reader;
mod reader_page;
mod reading_history;
mod screens;
mod typography;

pub use app::InkPaperApp;
pub use browser::{BrowseEntry, BrowseEntryKind, BrowseListing, BrowseRequest};
pub use reader::{
    ReaderChapter, ReaderChapterDirection, ReaderDocument, ReaderLoadError, ReaderPreferences,
    ReaderPreferencesError, ReaderPreferencesRequest, ReaderRequest, ReaderSession,
    load_adjacent_reader_chapter, load_reader_document,
};
pub use reading_history::{
    BookProgress, MAX_READING_HISTORY_ENTRIES, ReadingHistory, ReadingHistoryEntry,
    ReadingHistoryError, ReadingHistoryRequest,
};

pub use inkpaper_epub::SpineIndex;
