use inkpaper_reader::{ReaderSettings, Viewport};

mod document;
mod images;
mod loader;
mod measurer;
mod preferences;
mod progress;
mod state;

#[cfg(test)]
mod tests;

pub(crate) use document::{ReaderChapter, ReaderDocument};
pub(crate) use loader::{
    ReaderLoadError, ReaderSession, load_adjacent_reader_chapter, load_reader_document,
};
pub(crate) use preferences::{ReaderPreferences, ReaderPreferencesError, ReaderPreferencesRequest};
pub(crate) use state::{ReaderChapterDirection, ReaderRequest, ReaderState};

const READER_VIEWPORT_WIDTH: u32 = 440;
const READER_VIEWPORT_HEIGHT: u32 = 685;

const READER_FONT_SIZE_DEFAULT: u16 = 20;
const READER_FONT_SIZE_MIN: u16 = 14;
const READER_FONT_SIZE_MAX: u16 = 32;
const READER_FONT_SIZE_STEP: u16 = 2;

const READER_BLOCK_SPACING: u16 = 8;

pub(crate) fn reader_viewport() -> Viewport {
    Viewport::new(READER_VIEWPORT_WIDTH, READER_VIEWPORT_HEIGHT)
        .expect("reader viewport is statically non-zero")
}

fn reader_settings(font_size: u16) -> Option<ReaderSettings> {
    if !(READER_FONT_SIZE_MIN..=READER_FONT_SIZE_MAX).contains(&font_size) {
        return None;
    }

    ReaderSettings::new(font_size, READER_BLOCK_SPACING)
}

fn default_reader_settings() -> ReaderSettings {
    reader_settings(READER_FONT_SIZE_DEFAULT).expect("default reader settings are statically valid")
}
