use alloc::{string::String, vec::Vec};

use inkpaper_epub::{
    ContentOffset, Epub, EpubSource, Error as EpubError, FontWeight as ReaderFontWeight, SpineIndex,
};
use inkpaper_reader::{
    ImageMeasurer, Page, Pagination, ReaderSettings, TextMeasurer, TextStyle as ReaderTextStyle,
    Viewport, paginate_chapter,
};
use inkpaper_ui::{
    FontFamilyId, FontRegistry, FontRegistryError, FontWeight as UiFontWeight, ResolvedFont,
    ShapeError, ShapedGlyph, SimpleShaper,
};

const READER_VIEWPORT_WIDTH: u32 = 440;
const READER_VIEWPORT_HEIGHT: u32 = 685;
const READER_FONT_SIZE: u16 = 20;
const READER_BLOCK_SPACING: u16 = 8;
const READER_SHAPING_GLYPHS: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReaderRequest {
    OpenEpub(String),
}

#[derive(Debug)]
pub enum ReaderLoadError<E> {
    Epub(EpubError<E>),
    FontRegistry(FontRegistryError),
    Shape(ShapeError),
    SpineIndexOverflow,
    NoReadableChapter,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReaderDocument {
    path: String,
    title: Option<String>,
    creators: Vec<String>,
    package_path: String,
    chapter_path: String,
    spine: SpineIndex,
    spine_len: usize,
    pagination: Pagination<'static>,
}

impl ReaderDocument {
    #[allow(clippy::too_many_arguments)]
    fn new(
        path: String,
        title: Option<String>,
        creators: Vec<String>,
        package_path: String,
        chapter_path: String,
        spine: SpineIndex,
        spine_len: usize,
        pagination: Pagination<'static>,
    ) -> Self {
        Self {
            path,
            title,
            creators,
            package_path,
            chapter_path,
            spine,
            spine_len,
            pagination,
        }
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    pub fn creators(&self) -> &[String] {
        &self.creators
    }

    pub fn package_path(&self) -> &str {
        &self.package_path
    }

    pub fn chapter_path(&self) -> &str {
        &self.chapter_path
    }

    pub const fn spine(&self) -> SpineIndex {
        self.spine
    }

    pub const fn spine_len(&self) -> usize {
        self.spine_len
    }

    pub fn page_count(&self) -> usize {
        self.pagination.len()
    }

    pub fn page(&self, index: usize) -> Option<&Page<'static>> {
        self.pagination.pages().get(index)
    }

    pub fn first_page(&self) -> &Page<'static> {
        self.page(0)
            .expect("reader document always contains a page")
    }
}

pub(crate) fn reader_viewport() -> Viewport {
    Viewport::new(READER_VIEWPORT_WIDTH, READER_VIEWPORT_HEIGHT)
        .expect("reader viewport is statically non-zero")
}

fn reader_settings() -> ReaderSettings {
    ReaderSettings::new(READER_FONT_SIZE, READER_BLOCK_SPACING)
        .expect("reader settings are statically valid")
}

pub async fn load_reader_document<S>(
    path: String,
    source: S,
) -> Result<ReaderDocument, ReaderLoadError<S::Error>>
where
    S: EpubSource,
{
    let mut epub = Epub::open(source).await.map_err(ReaderLoadError::Epub)?;

    let title = epub.metadata().title().map(String::from);
    let creators = epub.metadata().creators().to_vec();
    let package_path = String::from(epub.package().path().as_str());
    let spine_len = epub.spine().items().len();

    for index in 0..spine_len {
        let linear = epub
            .spine()
            .items()
            .get(index)
            .is_some_and(|item| item.linear());

        if !linear {
            continue;
        }

        let is_xhtml = epub
            .package()
            .spine_manifest_item(index)
            .is_some_and(|item| item.media_type() == "application/xhtml+xml");

        if !is_xhtml {
            continue;
        }

        let Some(chapter) = epub
            .load_spine_chapter(index)
            .await
            .map_err(ReaderLoadError::Epub)?
        else {
            continue;
        };

        // cover/front-matter XHTML frequently contains only an image. Skip those for now.
        if chapter.content_len() == ContentOffset::ZERO {
            continue;
        }

        let styles = epub
            .load_chapter_styles(&chapter)
            .await
            .map_err(ReaderLoadError::Epub)?;

        let spine = SpineIndex::try_from_usize(index).ok_or(ReaderLoadError::SpineIndexOverflow)?;

        let chapter_path = String::from(chapter.path().as_str());

        // keep the shaping scratch out of the async state across EPUB I/O awaits.
        let mut measurer = ReaderMeasurer::new().map_err(ReaderLoadError::FontRegistry)?;

        let pagination = paginate_chapter(
            &chapter,
            &styles,
            spine,
            reader_viewport(),
            reader_settings(),
            &mut measurer,
        )
        .map_err(ReaderLoadError::Shape)?;

        // a chapter can contain textual source while CSS hides all of it.
        // don't choose such a chapter as the first thing the reader displays.
        if pagination
            .pages()
            .iter()
            .all(|page| page.items().is_empty())
        {
            continue;
        }

        return Ok(ReaderDocument::new(
            path,
            title,
            creators,
            package_path,
            chapter_path,
            spine,
            spine_len,
            pagination.into_owned(),
        ));
    }

    Err(ReaderLoadError::NoReadableChapter)
}

struct ReaderMeasurer {
    fonts: FontRegistry<'static, 1>,
    glyphs: [ShapedGlyph; READER_SHAPING_GLYPHS],
}

impl ReaderMeasurer {
    fn new() -> Result<Self, FontRegistryError> {
        let mut fonts = FontRegistry::default();

        let family = fonts.register_family()?;
        fonts.register_face(family, crate::typography::ui_font())?;

        Ok(Self {
            fonts,
            glyphs: [ShapedGlyph::EMPTY; READER_SHAPING_GLYPHS],
        })
    }

    fn resolve_font(&self, style: ReaderTextStyle) -> ResolvedFont<'static> {
        let weight = match style.font_weight() {
            ReaderFontWeight::Normal => UiFontWeight::NORMAL,
            ReaderFontWeight::Bold => UiFontWeight::BOLD,
        };

        self.fonts
            .resolve_family_weight(FontFamilyId::DEFAULT, weight)
            .expect("reader measurer always registers the UI font")
    }
}

impl TextMeasurer for ReaderMeasurer {
    type Error = ShapeError;

    fn measure_text(&mut self, text: &str, style: ReaderTextStyle) -> Result<u32, Self::Error> {
        let font = self.resolve_font(style);
        let shaper = SimpleShaper::with_properties(font.properties());

        let summary = shaper.measure(
            &self.fonts,
            font.id(),
            style.font_size(),
            text,
            &mut self.glyphs,
        )?;

        Ok(u32::try_from(summary.advance().non_negative().get()).unwrap_or(u32::MAX))
    }

    fn line_height(&mut self, style: ReaderTextStyle) -> Result<u32, Self::Error> {
        let font = self.resolve_font(style);

        Ok(u32::try_from(
            font.metrics(style.font_size())
                .line_height()
                .non_negative()
                .get(),
        )
        .unwrap_or(u32::MAX))
    }

    fn next_boundary(
        &mut self,
        text: &str,
        from: usize,
        style: ReaderTextStyle,
    ) -> Result<Option<usize>, Self::Error> {
        let font = self.resolve_font(style);
        let shaper = SimpleShaper::with_properties(font.properties());

        Ok(shaper.next_cluster_boundary(&self.fonts, font.id(), style.font_size(), text, from))
    }
}

impl ImageMeasurer for ReaderMeasurer {}

#[derive(Debug, Default)]
pub(crate) struct ReaderState {
    path: String,
    fallback_title: String,
    pending: Option<ReaderRequest>,
    document: Option<ReaderDocument>,
    page_index: usize,
    failed: bool,
}

impl ReaderState {
    pub(crate) fn open(&mut self, path: String, fallback_title: String) {
        self.pending = Some(ReaderRequest::OpenEpub(path.clone()));
        self.path = path;
        self.fallback_title = fallback_title;
        self.document = None;
        self.page_index = 0;
        self.failed = false;
    }

    pub(crate) fn take_request(&mut self) -> Option<ReaderRequest> {
        self.pending.take()
    }

    pub(crate) fn apply_document(&mut self, document: ReaderDocument) -> bool {
        if document.path() != self.path {
            return false;
        }

        self.document = Some(document);
        self.page_index = 0;
        self.failed = false;

        true
    }

    pub(crate) fn apply_error(&mut self, path: &str) -> bool {
        if path != self.path {
            return false;
        }

        self.document = None;
        self.page_index = 0;
        self.failed = true;

        true
    }

    pub(crate) fn previous_page(&mut self) -> bool {
        if self.document.is_none() || self.page_index == 0 {
            return false;
        }

        self.page_index -= 1;

        true
    }

    pub(crate) fn next_page(&mut self) -> bool {
        let Some(document) = self.document.as_ref() else {
            return false;
        };

        let next = self.page_index.saturating_add(1);

        if next >= document.page_count() {
            return false;
        }

        self.page_index = next;

        true
    }

    pub(crate) fn path(&self) -> &str {
        &self.path
    }

    pub(crate) fn title(&self) -> &str {
        self.document
            .as_ref()
            .and_then(ReaderDocument::title)
            .unwrap_or(&self.fallback_title)
    }

    pub(crate) fn creator(&self) -> &str {
        self.document
            .as_ref()
            .and_then(|document| document.creators().first())
            .map(String::as_str)
            .unwrap_or("")
    }

    pub(crate) fn page(&self) -> Option<&Page<'static>> {
        self.document
            .as_ref()
            .and_then(|document| document.page(self.page_index))
    }

    pub(crate) fn status(&self) -> &'static str {
        if self.failed {
            "Could not open EPUB"
        } else if self.document.is_some() {
            "EPUB opened"
        } else {
            "Opening EPUB..."
        }
    }

    pub(crate) fn detail(&self) -> &str {
        let Some(document) = &self.document else {
            return &self.path;
        };

        document.chapter_path()
    }
}

#[cfg(test)]
mod tests {
    use futures_lite::future;
    use inkpaper_epub::SliceSource;

    use super::*;

    #[test]
    fn real_epub_load_reaches_a_paginated_text_page() {
        let source = SliceSource::new(include_bytes!("../../../fixtures/book-boundaries.epub"));

        let document = future::block_on(load_reader_document(
            String::from("/Fixtures/book-boundaries.epub"),
            source,
        ))
        .unwrap();

        assert_eq!(document.path(), "/Fixtures/book-boundaries.epub");
        assert!(document.page_count() > 0);
        assert!(!document.first_page().items().is_empty());
        assert_eq!(document.first_page().start().spine(), document.spine());
    }

    #[test]
    fn reader_state_rejects_a_stale_document() {
        let source = SliceSource::new(include_bytes!("../../../fixtures/book-boundaries.epub"));

        let document = future::block_on(load_reader_document(
            String::from("/Fixtures/book-boundaries.epub"),
            source,
        ))
        .unwrap();

        let mut state = ReaderState::default();

        state.open(String::from("/Books/new.epub"), String::from("new"));

        assert!(!state.apply_document(document));
        assert_eq!(state.title(), "new");
        assert!(state.page().is_none());
    }

    #[test]
    fn reader_state_turns_pages_inside_loaded_pagination() {
        let path = String::from("/Fixtures/book-boundaries.epub");

        let source = SliceSource::new(include_bytes!("../../../fixtures/book-boundaries.epub"));

        let document = future::block_on(load_reader_document(path.clone(), source)).unwrap();

        let page_count = document.page_count();

        assert!(page_count > 0);

        let mut state = ReaderState::default();

        state.open(path, String::from("book-boundaries"));
        assert!(state.apply_document(document));

        assert_eq!(state.page_index, 0);
        assert!(state.page().is_some());
        assert!(!state.previous_page());

        for expected in 1..page_count {
            assert!(state.next_page());
            assert_eq!(state.page_index, expected);
            assert!(state.page().is_some());
        }

        assert!(!state.next_page());
        assert_eq!(state.page_index, page_count - 1);

        for expected in (0..page_count.saturating_sub(1)).rev() {
            assert!(state.previous_page());
            assert_eq!(state.page_index, expected);
            assert!(state.page().is_some());
        }

        assert_eq!(state.page_index, 0);
        assert!(!state.previous_page());
    }
}
