use alloc::{borrow::Cow, vec::Vec};

use inkpaper_epub::{BookLocation, ChapterImage, LinkTarget};

use crate::{ReadingPosition, metrics::TextStyle};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextFragment<'a> {
    text: Cow<'a, str>,
    bounds: Rect,
    style: TextStyle,
    link: Option<Cow<'a, LinkTarget>>,
}

impl<'a> TextFragment<'a> {
    pub(crate) const fn new(
        text: &'a str,
        bounds: Rect,
        style: TextStyle,
        link: Option<&'a LinkTarget>,
    ) -> Self {
        Self {
            text: Cow::Borrowed(text),
            bounds,
            style,
            link: match link {
                Some(link) => Some(Cow::Borrowed(link)),
                None => None,
            },
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub const fn bounds(&self) -> Rect {
        self.bounds
    }

    pub const fn style(&self) -> TextStyle {
        self.style
    }

    pub fn link(&self) -> Option<&LinkTarget> {
        self.link.as_deref()
    }

    pub fn into_owned(self) -> TextFragment<'static> {
        TextFragment {
            text: Cow::Owned(self.text.into_owned()),
            bounds: self.bounds,
            style: self.style,
            link: self.link.map(|link| Cow::Owned(link.into_owned())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageFragment<'a> {
    image: Cow<'a, ChapterImage>,
    bounds: Rect,
}

impl<'a> ImageFragment<'a> {
    pub(crate) const fn new(image: &'a ChapterImage, bounds: Rect) -> Self {
        Self {
            image: Cow::Borrowed(image),
            bounds,
        }
    }

    pub fn image(&self) -> &ChapterImage {
        &self.image
    }

    pub fn into_owned(self) -> ImageFragment<'static> {
        ImageFragment {
            image: Cow::Owned(self.image.into_owned()),
            bounds: self.bounds,
        }
    }

    pub const fn bounds(&self) -> Rect {
        self.bounds
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
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

    pub fn into_owned(self) -> PageItem<'static> {
        match self {
            Self::Text(fragment) => PageItem::Text(fragment.into_owned()),
            Self::Image(fragment) => PageItem::Image(fragment.into_owned()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page<'a> {
    position: ReadingPosition,
    end_position: ReadingPosition,
    items: Vec<PageItem<'a>>,
}

impl<'a> Page<'a> {
    pub(crate) fn new(
        position: ReadingPosition,
        end_position: ReadingPosition,
        items: Vec<PageItem<'a>>,
    ) -> Self {
        Self {
            position,
            end_position,
            items,
        }
    }

    pub const fn range(&self) -> PageRange {
        PageRange::new(self.start(), self.end())
    }

    pub const fn start(&self) -> BookLocation {
        self.position.location()
    }

    pub const fn end(&self) -> BookLocation {
        self.end_position.location()
    }

    /// the start of this page, including its position among non-text content
    pub const fn position(&self) -> ReadingPosition {
        self.position
    }

    /// the exclusive end of the flow assigned to this page
    pub const fn end_position(&self) -> ReadingPosition {
        self.end_position
    }

    pub fn items(&self) -> &[PageItem<'a>] {
        &self.items
    }

    pub(crate) fn set_end(&mut self, end: ReadingPosition) {
        self.end_position = end;
    }

    pub fn into_owned(self) -> Page<'static> {
        Page {
            position: self.position,
            end_position: self.end_position,
            items: self.items.into_iter().map(PageItem::into_owned).collect(),
        }
    }
}
