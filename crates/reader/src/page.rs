use alloc::vec::Vec;

use inkpaper_epub::{BookLocation, ChapterImage, LinkTarget};

use crate::metrics::TextStyle;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

impl Rect {
    pub const fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub const fn x(self) -> u32 {
        self.x
    }

    pub const fn y(self) -> u32 {
        self.y
    }

    pub const fn width(self) -> u32 {
        self.width
    }

    pub const fn height(self) -> u32 {
        self.height
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageRange {
    start: BookLocation,
    end: BookLocation,
}

impl PageRange {
    pub const fn new(start: BookLocation, end: BookLocation) -> Self {
        Self { start, end }
    }

    pub const fn start(self) -> BookLocation {
        self.start
    }

    pub const fn end(self) -> BookLocation {
        self.end
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextFragment<'a> {
    text: &'a str,
    bounds: Rect,
    style: TextStyle,
    link: Option<&'a LinkTarget>,
}

impl<'a> TextFragment<'a> {
    pub(crate) const fn new(
        text: &'a str,
        bounds: Rect,
        style: TextStyle,
        link: Option<&'a LinkTarget>,
    ) -> Self {
        Self {
            text,
            bounds,
            style,
            link,
        }
    }

    pub const fn text(&self) -> &'a str {
        self.text
    }

    pub const fn bounds(&self) -> Rect {
        self.bounds
    }

    pub const fn style(&self) -> TextStyle {
        self.style
    }

    pub const fn link(&self) -> Option<&'a LinkTarget> {
        self.link
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageFragment<'a> {
    image: &'a ChapterImage,
    bounds: Rect,
}

impl<'a> ImageFragment<'a> {
    pub(crate) const fn new(image: &'a ChapterImage, bounds: Rect) -> Self {
        Self { image, bounds }
    }

    pub const fn image(&self) -> &'a ChapterImage {
        self.image
    }

    pub const fn bounds(&self) -> Rect {
        self.bounds
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageItem<'a> {
    Text(TextFragment<'a>),
    Image(ImageFragment<'a>),
}

impl PageItem<'_> {
    pub const fn bounds(&self) -> Rect {
        match self {
            Self::Text(fragment) => fragment.bounds(),
            Self::Image(fragment) => fragment.bounds(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page<'a> {
    range: PageRange,
    items: Vec<PageItem<'a>>,
}

impl<'a> Page<'a> {
    pub(crate) fn new(range: PageRange, items: Vec<PageItem<'a>>) -> Self {
        Self { range, items }
    }

    pub const fn range(&self) -> PageRange {
        self.range
    }

    pub const fn start(&self) -> BookLocation {
        self.range.start()
    }

    pub const fn end(&self) -> BookLocation {
        self.range.end()
    }

    pub fn items(&self) -> &[PageItem<'a>] {
        &self.items
    }

    pub(crate) fn set_end(&mut self, end: BookLocation) {
        self.range = PageRange::new(self.range.start(), end);
    }
}
