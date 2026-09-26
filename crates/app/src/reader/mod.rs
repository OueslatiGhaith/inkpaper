use inkpaper_reader::{ReaderSettings, Viewport};

mod document;
mod images;
mod loader;
mod measure_cache;
mod measurer;
mod preferences;
mod progress;
mod state;
mod toc;

#[cfg(test)]
mod tests;

pub(crate) use document::{ReaderChapter, ReaderDocument};
pub(crate) use loader::{
    ReaderLoadError, ReaderSession, load_adjacent_reader_chapter, load_reader_document,
};
pub(crate) use preferences::{ReaderPreferences, ReaderPreferencesError, ReaderPreferencesRequest};
pub(crate) use state::{ReaderChapterDirection, ReaderMenuTab, ReaderRequest, ReaderState};
pub(crate) use toc::{TableOfContents, TocEntry};

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

/// Maps a slider value from 0 to 100 to the nearest supported font size.
pub(crate) fn font_size_from_slider(value: u8) -> u16 {
    let steps = u32::from((READER_FONT_SIZE_MAX - READER_FONT_SIZE_MIN) / READER_FONT_SIZE_STEP);
    let step = (u32::from(value.min(100)) * steps + 50) / 100;

    READER_FONT_SIZE_MIN + u16::try_from(step).unwrap_or(0) * READER_FONT_SIZE_STEP
}

/// Where `font_size` sits on a slider from 0 to 100.
pub(crate) fn font_size_slider_value(font_size: u16) -> u8 {
    let range = u32::from(READER_FONT_SIZE_MAX - READER_FONT_SIZE_MIN);
    let offset = u32::from(
        font_size.clamp(READER_FONT_SIZE_MIN, READER_FONT_SIZE_MAX) - READER_FONT_SIZE_MIN,
    );

    u8::try_from(offset * 100 / range).unwrap_or(100)
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
