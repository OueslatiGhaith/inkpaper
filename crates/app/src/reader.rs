use alloc::{format, string::String, vec::Vec};

use inkpaper_epub::{
    ContentOffset, Epub, EpubSource, Error as EpubError, FontWeight as ReaderFontWeight, SpineIndex,
};
use inkpaper_reader::{
    ImageMeasurer, Page, Pagination, ReaderSettings, ReadingPosition, TextMeasurer,
    TextStyle as ReaderTextStyle, Viewport, paginate_chapter,
};
use inkpaper_ui::{
    FontFamilyId, FontRegistry, FontRegistryError, FontWeight as UiFontWeight, ResolvedFont,
    ShapeError, ShapedGlyph, SimpleShaper,
};

use crate::ReadingProgress;

const READER_VIEWPORT_WIDTH: u32 = 440;
const READER_VIEWPORT_HEIGHT: u32 = 685;
const READER_FONT_SIZE: u16 = 20;
const READER_BLOCK_SPACING: u16 = 8;
const READER_SHAPING_GLYPHS: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReaderChapterDirection {
    Previous,
    Next,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReaderRequest {
    OpenEpub(String),
    LoadAdjacentChapter {
        path: String,
        from: SpineIndex,
        direction: ReaderChapterDirection,
    },
    UpdateProgress(ReadingProgress),
}

#[derive(Debug)]
pub enum ReaderLoadError<E> {
    Epub(EpubError<E>),
    FontRegistry(FontRegistryError),
    Shape(ShapeError),
    SpineIndexOverflow,
    NoReadableChapter,
}

pub struct ReaderSession<S>
where
    S: EpubSource,
{
    path: String,
    epub: Epub<S>,
}

impl<S> ReaderSession<S>
where
    S: EpubSource,
{
    pub async fn open(path: String, source: S) -> Result<Self, ReaderLoadError<S::Error>> {
        let epub = Epub::open(source).await.map_err(ReaderLoadError::Epub)?;

        Ok(Self { path, epub })
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn identifier(&self) -> Option<&str> {
        self.epub.metadata().identifier()
    }

    pub async fn load_document(&mut self) -> Result<ReaderDocument, ReaderLoadError<S::Error>> {
        self.load_document_at(None).await
    }

    pub async fn load_document_at(
        &mut self,
        position: Option<ReadingPosition>,
    ) -> Result<ReaderDocument, ReaderLoadError<S::Error>> {
        load_reader_document_from_epub(self.path.clone(), &mut self.epub, position).await
    }

    pub async fn load_adjacent_chapter(
        &mut self,
        from: SpineIndex,
        direction: ReaderChapterDirection,
    ) -> Result<Option<ReaderChapter>, ReaderLoadError<S::Error>> {
        load_adjacent_reader_chapter_from_epub(&mut self.epub, from, direction).await
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReaderChapter {
    chapter_path: String,
    spine: SpineIndex,
    pagination: Pagination<'static>,
}

impl ReaderChapter {
    fn new(chapter_path: String, spine: SpineIndex, pagination: Pagination<'static>) -> Self {
        Self {
            chapter_path,
            spine,
            pagination,
        }
    }

    pub fn chapter_path(&self) -> &str {
        &self.chapter_path
    }

    pub const fn spine(&self) -> SpineIndex {
        self.spine
    }

    pub fn page_count(&self) -> usize {
        self.pagination.len()
    }

    pub fn page(&self, index: usize) -> Option<&Page<'static>> {
        self.pagination.pages().get(index)
    }

    pub fn page_at_position(&self, position: ReadingPosition) -> Option<usize> {
        self.pagination.page_at_position(position)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReaderDocument {
    path: String,
    identifier: Option<String>,
    title: Option<String>,
    creators: Vec<String>,
    package_path: String,
    spine_len: usize,
    chapter: ReaderChapter,
    opening_page_index: usize,
}

impl ReaderDocument {
    #[allow(clippy::too_many_arguments)]
    fn new(
        path: String,
        identifier: Option<String>,
        title: Option<String>,
        creators: Vec<String>,
        package_path: String,
        spine_len: usize,
        chapter: ReaderChapter,
        opening_page_index: usize,
    ) -> Self {
        Self {
            path,
            identifier,
            title,
            creators,
            package_path,
            spine_len,
            chapter,
            opening_page_index,
        }
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn identifier(&self) -> Option<&str> {
        self.identifier.as_deref()
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
        self.chapter.chapter_path()
    }

    pub const fn spine(&self) -> SpineIndex {
        self.chapter.spine()
    }

    pub const fn spine_len(&self) -> usize {
        self.spine_len
    }

    pub fn page_count(&self) -> usize {
        self.chapter.page_count()
    }

    pub fn page(&self, index: usize) -> Option<&Page<'static>> {
        self.chapter.page(index)
    }

    pub fn first_page(&self) -> &Page<'static> {
        self.page(0)
            .expect("reader document always contains a page")
    }

    pub const fn opening_page_index(&self) -> usize {
        self.opening_page_index
    }

    fn replace_chapter(&mut self, chapter: ReaderChapter) {
        self.chapter = chapter;
        self.opening_page_index = 0;
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
    let mut session = ReaderSession::open(path, source).await?;
    session.load_document().await
}

pub async fn load_adjacent_reader_chapter<S>(
    source: S,
    from: SpineIndex,
    direction: ReaderChapterDirection,
) -> Result<Option<ReaderChapter>, ReaderLoadError<S::Error>>
where
    S: EpubSource,
{
    let mut epub = Epub::open(source).await.map_err(ReaderLoadError::Epub)?;
    load_adjacent_reader_chapter_from_epub(&mut epub, from, direction).await
}

async fn load_reader_document_from_epub<S>(
    path: String,
    epub: &mut Epub<S>,
    resume: Option<ReadingPosition>,
) -> Result<ReaderDocument, ReaderLoadError<S::Error>>
where
    S: EpubSource,
{
    let identifier = epub.metadata().identifier().map(String::from);

    let title = epub.metadata().title().map(String::from);
    let creators = epub.metadata().creators().to_vec();

    let package_path = String::from(epub.package().path().as_str());

    let spine_len = epub.spine().items().len();

    if let Some(position) = resume
        && let Some(index) = position.location().spine().as_usize()
        && index < spine_len
        && let Some(chapter) = load_readable_chapter_at(epub, index).await?
    {
        let opening_page_index = chapter.page_at_position(position).unwrap_or(0);

        return Ok(ReaderDocument::new(
            path,
            identifier,
            title,
            creators,
            package_path,
            spine_len,
            chapter,
            opening_page_index,
        ));
    }

    for index in 0..spine_len {
        if let Some(chapter) = load_readable_chapter_at(epub, index).await? {
            return Ok(ReaderDocument::new(
                path,
                identifier,
                title,
                creators,
                package_path,
                spine_len,
                chapter,
                0,
            ));
        }
    }

    Err(ReaderLoadError::NoReadableChapter)
}

async fn load_adjacent_reader_chapter_from_epub<S>(
    epub: &mut Epub<S>,
    from: SpineIndex,
    direction: ReaderChapterDirection,
) -> Result<Option<ReaderChapter>, ReaderLoadError<S::Error>>
where
    S: EpubSource,
{
    let spine_len = epub.spine().items().len();
    let from = from.as_usize().ok_or(ReaderLoadError::SpineIndexOverflow)?;

    if from >= spine_len {
        return Ok(None);
    }

    match direction {
        ReaderChapterDirection::Next => {
            for index in from.saturating_add(1)..spine_len {
                if let Some(chapter) = load_readable_chapter_at(epub, index).await? {
                    return Ok(Some(chapter));
                }
            }
        }
        ReaderChapterDirection::Previous => {
            for index in (0..from).rev() {
                if let Some(chapter) = load_readable_chapter_at(epub, index).await? {
                    return Ok(Some(chapter));
                }
            }
        }
    }

    Ok(None)
}

async fn load_readable_chapter_at<S>(
    epub: &mut Epub<S>,
    index: usize,
) -> Result<Option<ReaderChapter>, ReaderLoadError<S::Error>>
where
    S: EpubSource,
{
    let linear = epub
        .spine()
        .items()
        .get(index)
        .is_some_and(|item| item.linear());

    if !linear {
        return Ok(None);
    }

    let is_xhtml = epub
        .package()
        .spine_manifest_item(index)
        .is_some_and(|item| item.media_type() == "application/xhtml+xml");

    if !is_xhtml {
        return Ok(None);
    }

    let Some(chapter) = epub
        .load_spine_chapter(index)
        .await
        .map_err(ReaderLoadError::Epub)?
    else {
        return Ok(None);
    };

    // image-only covers/front matter are deferred until EPUB image rendering is implemented.
    if chapter.content_len() == ContentOffset::ZERO {
        return Ok(None);
    }

    let styles = epub
        .load_chapter_styles(&chapter)
        .await
        .map_err(ReaderLoadError::Epub)?;

    let spine = SpineIndex::try_from_usize(index).ok_or(ReaderLoadError::SpineIndexOverflow)?;

    let chapter_path = String::from(chapter.path().as_str());

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

    if pagination
        .pages()
        .iter()
        .all(|page| page.items().is_empty())
    {
        return Ok(None);
    }

    Ok(Some(ReaderChapter::new(
        chapter_path,
        spine,
        pagination.into_owned(),
    )))
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PendingChapterRequest {
    from: SpineIndex,
    direction: ReaderChapterDirection,
}

#[derive(Debug, Default)]
struct ReaderChromeState {
    controls_visible: bool,
    page_label: String,
    section_label: String,
}

#[derive(Debug, Default)]
pub(crate) struct ReaderState {
    path: String,
    fallback_title: String,
    pending: Option<ReaderRequest>,
    pending_chapter: Option<PendingChapterRequest>,
    document: Option<ReaderDocument>,
    page_index: usize,
    failed: bool,
    chrome: ReaderChromeState,
}

impl ReaderState {
    pub(crate) fn open(&mut self, path: String, fallback_title: String) {
        self.pending = Some(ReaderRequest::OpenEpub(path.clone()));

        self.pending_chapter = None;
        self.path = path;
        self.fallback_title = fallback_title;
        self.document = None;
        self.page_index = 0;
        self.failed = false;
        self.chrome = ReaderChromeState::default();
    }

    pub(crate) fn take_request(&mut self) -> Option<ReaderRequest> {
        self.pending.take()
    }

    pub(crate) fn apply_document(&mut self, document: ReaderDocument) -> bool {
        if document.path() != self.path {
            return false;
        }

        let page_index = document
            .opening_page_index()
            .min(document.page_count().saturating_sub(1));

        self.document = Some(document);
        self.pending_chapter = None;
        self.page_index = page_index;
        self.failed = false;
        self.chrome.controls_visible = false;

        self.refresh_chrome();
        self.queue_progress_update();

        true
    }

    pub(crate) fn apply_error(&mut self, path: &str) -> bool {
        if path != self.path {
            return false;
        }

        self.document = None;
        self.pending_chapter = None;
        self.page_index = 0;
        self.failed = true;
        self.chrome = ReaderChromeState::default();

        true
    }

    pub(crate) fn apply_chapter(
        &mut self,
        path: &str,
        from: SpineIndex,
        direction: ReaderChapterDirection,
        chapter: ReaderChapter,
    ) -> bool {
        if path != self.path {
            return false;
        }

        let expected = PendingChapterRequest { from, direction };

        if self.pending_chapter != Some(expected) {
            return false;
        }

        let Some(document) = self.document.as_mut() else {
            self.pending_chapter = None;
            return false;
        };

        if document.spine() != from {
            self.pending_chapter = None;
            return false;
        }

        let valid_direction = match direction {
            ReaderChapterDirection::Next => chapter.spine() > from,

            ReaderChapterDirection::Previous => chapter.spine() < from,
        };

        if !valid_direction {
            self.pending_chapter = None;
            return false;
        }

        document.replace_chapter(chapter);

        self.page_index = match direction {
            ReaderChapterDirection::Next => 0,

            ReaderChapterDirection::Previous => document.page_count().saturating_sub(1),
        };

        self.pending_chapter = None;
        self.failed = false;
        self.chrome.controls_visible = false;

        self.refresh_chrome();
        self.queue_progress_update();

        true
    }

    pub(crate) fn finish_chapter_request(
        &mut self,
        path: &str,
        from: SpineIndex,
        direction: ReaderChapterDirection,
    ) -> bool {
        if path != self.path {
            return false;
        }

        let expected = PendingChapterRequest { from, direction };

        if self.pending_chapter != Some(expected) {
            return false;
        }

        self.pending_chapter = None;

        true
    }

    pub(crate) fn previous_page(&mut self) -> bool {
        let Some(document) = self.document.as_ref() else {
            return false;
        };

        if self.page_index > 0 {
            self.page_index -= 1;
            self.chrome.controls_visible = false;

            self.refresh_chrome();
            self.queue_progress_update();

            return true;
        }

        let from = document.spine();

        self.request_adjacent_chapter(from, ReaderChapterDirection::Previous);

        false
    }

    pub(crate) fn next_page(&mut self) -> bool {
        let Some(document) = self.document.as_ref() else {
            return false;
        };

        let page_count = document.page_count();

        let from = document.spine();

        let next = self.page_index.saturating_add(1);

        if next < page_count {
            self.page_index = next;
            self.chrome.controls_visible = false;

            self.refresh_chrome();
            self.queue_progress_update();

            return true;
        }

        self.request_adjacent_chapter(from, ReaderChapterDirection::Next);

        false
    }

    fn request_adjacent_chapter(&mut self, from: SpineIndex, direction: ReaderChapterDirection) {
        if self.pending_chapter.is_some() {
            return;
        }

        let request = PendingChapterRequest { from, direction };

        self.pending_chapter = Some(request);

        self.pending = Some(ReaderRequest::LoadAdjacentChapter {
            path: self.path.clone(),
            from,
            direction,
        });
    }

    pub(crate) fn toggle_controls(&mut self) -> bool {
        if self.document.is_none() {
            return false;
        }

        self.chrome.controls_visible = !self.chrome.controls_visible;

        true
    }

    pub(crate) fn controls_visible(&self) -> bool {
        self.chrome.controls_visible
    }

    pub(crate) fn reading_position(&self) -> Option<ReadingPosition> {
        self.page().map(Page::position)
    }

    pub(crate) fn page_label(&self) -> &str {
        &self.chrome.page_label
    }

    pub(crate) fn section_label(&self) -> &str {
        &self.chrome.section_label
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

    fn refresh_chrome(&mut self) {
        self.chrome.page_label.clear();
        self.chrome.section_label.clear();

        let Some(document) = self.document.as_ref() else {
            return;
        };

        let Some(page) = document.page(self.page_index) else {
            return;
        };

        let page_count = document.page_count();

        if page_count == 0 {
            return;
        }

        let page_number = self.page_index.saturating_add(1).min(page_count);

        let chapter_end = document
            .page(page_count.saturating_sub(1))
            .map(Page::end_position);

        let chapter_percent = match chapter_end {
            Some(end) => {
                let current_offset = page.position().location().offset().get();
                let end_offset = end.location().offset().get();

                if end_offset == 0 {
                    0
                } else {
                    let current = current_offset.min(end_offset);

                    u8::try_from((u128::from(current) * 100) / u128::from(end_offset))
                        .unwrap_or(100)
                        .min(100)
                }
            }

            None => 0,
        };

        let section_number = document.spine().get().saturating_add(1);

        self.chrome.page_label = format!("{} / {}", page_number, page_count);
        self.chrome.section_label = format!(
            "S {} / {}  {}%",
            section_number,
            document.spine_len(),
            chapter_percent,
        );
    }

    pub(crate) fn current_progress(&self) -> Option<ReadingProgress> {
        let document = self.document.as_ref()?;

        let page = document.page(self.page_index)?;

        Some(ReadingProgress::new(
            self.path.clone(),
            document.identifier().map(String::from),
            page.position(),
        ))
    }

    fn queue_progress_update(&mut self) {
        let Some(progress) = self.current_progress() else {
            return;
        };

        self.pending = Some(ReaderRequest::UpdateProgress(progress));
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

    #[test]
    fn next_chapter_request_replaces_chapter_and_lands_on_first_page() {
        let path = String::from("/Fixtures/book-boundaries.epub");

        let source = SliceSource::new(include_bytes!("../../../fixtures/book-boundaries.epub"));

        let document = future::block_on(load_reader_document(path.clone(), source)).unwrap();

        let from = document.spine();

        let next_spine = SpineIndex::new(from.get().saturating_add(1));

        let next_chapter = ReaderChapter::new(
            String::from("Text/next.xhtml"),
            next_spine,
            document.chapter.pagination.clone(),
        );

        let last_page = document.page_count().saturating_sub(1);

        let mut state = ReaderState::default();

        state.open(path.clone(), String::from("book-boundaries"));

        assert!(state.apply_document(document));

        state.page_index = last_page;

        assert!(!state.next_page());
        assert_eq!(
            state.take_request(),
            Some(ReaderRequest::LoadAdjacentChapter {
                path: path.clone(),
                from,
                direction: ReaderChapterDirection::Next,
            }),
        );

        assert!(state.apply_chapter(&path, from, ReaderChapterDirection::Next, next_chapter));
        assert_eq!(state.page_index, 0);
        assert_eq!(state.document.as_ref().unwrap().spine(), next_spine);
        assert!(state.page().is_some());
    }

    #[test]
    fn reader_session_supports_multiple_operations_on_one_epub() {
        let path = String::from("/Fixtures/book-boundaries.epub");

        let source = SliceSource::new(include_bytes!("../../../fixtures/book-boundaries.epub"));

        let mut session = future::block_on(ReaderSession::open(path.clone(), source)).unwrap();

        assert_eq!(session.path(), path);

        let document = future::block_on(session.load_document()).unwrap();

        assert_eq!(document.path(), "/Fixtures/book-boundaries.epub");

        let adjacent = future::block_on(
            session.load_adjacent_chapter(document.spine(), ReaderChapterDirection::Next),
        );

        assert!(adjacent.is_ok());
    }

    #[test]
    fn reader_chrome_tracks_the_current_reading_position() {
        let path = String::from("/Fixtures/book-boundaries.epub");

        let source = SliceSource::new(include_bytes!("../../../fixtures/book-boundaries.epub"));

        let document = future::block_on(load_reader_document(path.clone(), source)).unwrap();

        let page_count = document.page_count();

        let expected_position = document.first_page().position();

        let mut state = ReaderState::default();

        state.open(path, String::from("book-boundaries"));

        assert!(state.apply_document(document));

        let expected_page_label = format!("1 / {}", page_count);

        assert_eq!(state.page_label(), expected_page_label.as_str());

        assert_eq!(state.reading_position(), Some(expected_position));

        assert!(state.section_label().starts_with("S "));
    }

    #[test]
    fn reader_controls_toggle_only_after_a_book_is_loaded() {
        let mut state = ReaderState::default();

        assert!(!state.toggle_controls());
        assert!(!state.controls_visible());

        let path = String::from("/Fixtures/book-boundaries.epub");

        let source = SliceSource::new(include_bytes!("../../../fixtures/book-boundaries.epub"));

        let document = future::block_on(load_reader_document(path.clone(), source)).unwrap();

        state.open(path, String::from("book-boundaries"));

        assert!(state.apply_document(document));

        assert!(!state.controls_visible());

        assert!(state.toggle_controls());
        assert!(state.controls_visible());

        assert!(state.toggle_controls());
        assert!(!state.controls_visible());
    }

    #[test]
    fn reader_session_reopens_at_saved_position() {
        let path = String::from("/Fixtures/book-boundaries.epub");

        let source = SliceSource::new(include_bytes!("../../../fixtures/book-boundaries.epub"));

        let document = future::block_on(load_reader_document(path.clone(), source)).unwrap();

        let target_index = document.page_count().saturating_sub(1);

        let target_position = document.page(target_index).unwrap().position();

        let source = SliceSource::new(include_bytes!("../../../fixtures/book-boundaries.epub"));

        let mut session = future::block_on(ReaderSession::open(path, source)).unwrap();

        let resumed = future::block_on(session.load_document_at(Some(target_position))).unwrap();

        assert_eq!(resumed.opening_page_index(), target_index);

        assert_eq!(
            resumed
                .page(resumed.opening_page_index())
                .unwrap()
                .position(),
            target_position,
        );
    }
}
