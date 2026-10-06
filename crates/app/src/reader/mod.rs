mod document;
mod images;
mod loader;
mod measure_cache;
mod measurer;
mod preferences;
mod progress;
mod state;
mod text_settings;
mod toc;

#[cfg(test)]
mod tests;

pub(crate) use document::{ReaderChapter, ReaderDocument};
pub(crate) use loader::ReaderSession;
#[cfg(test)]
pub(crate) use loader::load_reader_document;
pub(crate) use preferences::{ReaderPreferences, ReaderPreferencesRequest};
pub(crate) use state::{ReaderChapterDirection, ReaderMenuTab, ReaderRequest, ReaderState};
pub(crate) use text_settings::{LineSpacing, PageBounds, ScreenMargin, TextSetting, TextSettings};
pub(crate) use toc::{TableOfContents, TocEntry};

const READER_FONT_SIZE_DEFAULT: u16 = 20;
const READER_FONT_SIZE_MIN: u16 = 14;
const READER_FONT_SIZE_MAX: u16 = 32;
const READER_FONT_SIZE_STEP: u16 = 2;

const READER_BLOCK_SPACING: u16 = 8;
