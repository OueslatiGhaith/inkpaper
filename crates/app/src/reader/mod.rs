use inkpaper_reader::{ReaderSettings, Viewport};

mod document;
mod loader;
mod measurer;
mod progress;
mod state;

#[cfg(test)]
mod tests;

pub use document::{ReaderChapter, ReaderDocument};
pub use loader::{
    ReaderLoadError, ReaderSession, load_adjacent_reader_chapter, load_reader_document,
};
pub use state::{ReaderChapterDirection, ReaderRequest};

pub(crate) use state::ReaderState;

const READER_VIEWPORT_WIDTH: u32 = 440;
const READER_VIEWPORT_HEIGHT: u32 = 685;
const READER_FONT_SIZE: u16 = 20;
const READER_BLOCK_SPACING: u16 = 8;

pub(crate) fn reader_viewport() -> Viewport {
    Viewport::new(READER_VIEWPORT_WIDTH, READER_VIEWPORT_HEIGHT)
        .expect("reader viewport is statically non-zero")
}

fn reader_settings() -> ReaderSettings {
    ReaderSettings::new(READER_FONT_SIZE, READER_BLOCK_SPACING)
        .expect("reader settings are statically valid")
}
