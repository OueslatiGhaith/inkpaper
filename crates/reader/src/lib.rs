#![no_std]

extern crate alloc;

mod metrics;
mod page;
mod pagination;
mod position;

pub use inkpaper_epub::{
    BlockKind, ChapterImage, FontStyle, FontWeight, ImageDimensions, SpineIndex,
};
pub use metrics::{ImageMeasurer, TextMeasurer, TextStyle};
pub use page::{ImageFragment, Page, PageItem, PageRange, Rect, TextFragment};
pub use pagination::{Pagination, paginate_chapter};
pub use position::ReadingPosition;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Viewport {
    width: u32,
    height: u32,
}

impl Viewport {
    pub const fn new(width: u32, height: u32) -> Option<Self> {
        if width == 0 || height == 0 {
            return None;
        }

        Some(Self { width, height })
    }

    pub const fn width(self) -> u32 {
        self.width
    }

    pub const fn height(self) -> u32 {
        self.height
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReaderSettings {
    font_size: u16,
    block_spacing: u16,
    line_height_percent: Option<u16>,
}

impl ReaderSettings {
    pub const fn new(font_size: u16, block_spacing: u16) -> Option<Self> {
        if font_size == 0 {
            return None;
        }

        Some(Self {
            font_size,
            block_spacing,
            line_height_percent: None,
        })
    }

    /// Lines `percent` of the font size tall, instead of the book's CSS
    /// `line-height` or the font's own line height.
    pub const fn with_line_height_percent(self, percent: u16) -> Option<Self> {
        if percent == 0 {
            return None;
        }

        Some(Self {
            line_height_percent: Some(percent),
            ..self
        })
    }

    pub const fn font_size(self) -> u16 {
        self.font_size
    }

    pub const fn block_spacing(self) -> u16 {
        self.block_spacing
    }

    pub const fn line_height_percent(self) -> Option<u16> {
        self.line_height_percent
    }
}
