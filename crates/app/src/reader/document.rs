use alloc::{string::String, vec::Vec};

use inkpaper_epub::{ArchivePath, Chapter, ChapterStyles, SpineIndex};
use inkpaper_reader::{
    ChapterPageIndex, Page, ReaderSettings, ReadingPosition, Viewport, render_page,
};
use inkpaper_ui::{ImageId, ImageSource, ResourceRuntimeApi};

use crate::{
    BookProgress,
    reader::{
        images::{ChapterImageMetrics, ChapterImages, page_image_paths},
        measurer::{ReaderFonts, ReaderMeasurer},
        progress::BookProgressMap,
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReaderChapter {
    chapter_path: String,
    spine: SpineIndex,
    chapter: Chapter,
    styles: ChapterStyles,
    image_metrics: ChapterImageMetrics,
    viewport: Viewport,
    settings: ReaderSettings,
    index: ChapterPageIndex,
    current_page_index: usize,
    current_page: Page<'static>,
    images: ChapterImages,
}

impl ReaderChapter {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn from_index(
        chapter_path: String,
        spine: SpineIndex,
        chapter: Chapter,
        styles: ChapterStyles,
        image_metrics: ChapterImageMetrics,
        viewport: Viewport,
        settings: ReaderSettings,
        index: ChapterPageIndex,
        current_page_index: usize,
        current_page: Page<'static>,
        images: ChapterImages,
    ) -> Self {
        Self {
            chapter_path,
            spine,
            chapter,
            styles,
            image_metrics,
            viewport,
            settings,
            index,
            current_page_index,
            current_page,
            images,
        }
    }

    #[cfg(test)]
    pub(super) fn with_path_and_spine(&self, chapter_path: String, spine: SpineIndex) -> Self {
        let mut cloned = self.clone();
        cloned.chapter_path = chapter_path;
        cloned.spine = spine;
        cloned
    }

    pub(crate) fn register_images<'resource>(
        &mut self,
        runtime: &mut impl ResourceRuntimeApi<'resource>,
        max_slots: usize,
        registered: &mut Vec<ImageId>,
    ) {
        self.images.register(runtime, max_slots, registered);
    }

    pub(super) fn replace_images(&mut self, images: ChapterImages) {
        self.images = images;
    }

    pub(super) fn missing_page_images(&self) -> Vec<ArchivePath> {
        page_image_paths(&self.current_page)
            .into_iter()
            .filter(|path| !self.images.contains(path))
            .collect()
    }

    pub fn image_source(&self, path: &ArchivePath) -> Option<ImageSource> {
        self.images.source(path)
    }

    pub fn chapter_path(&self) -> &str {
        &self.chapter_path
    }

    pub const fn spine(&self) -> SpineIndex {
        self.spine
    }

    pub fn page_count(&self) -> usize {
        self.index.len()
    }

    #[cfg(test)]
    pub fn page_position(&self, index: usize) -> Option<ReadingPosition> {
        self.index.page_position(index)
    }

    pub fn page_end_position(&self, index: usize) -> Option<ReadingPosition> {
        self.index.page_end_position(index)
    }

    pub const fn end_position(&self) -> ReadingPosition {
        self.index.end_position()
    }

    pub fn page(&self, index: usize) -> Option<&Page<'static>> {
        (index == self.current_page_index).then_some(&self.current_page)
    }

    /// Materializes `index` on demand from its checkpoint when it is not
    /// already the cached visible page.
    pub(super) fn select_page(&mut self, index: usize, fonts: &ReaderFonts) -> bool {
        if index >= self.index.len() {
            return false;
        }

        if index == self.current_page_index {
            return true;
        }

        let mut measurer = ReaderMeasurer::new(fonts, self.image_metrics.clone());
        let Ok(Some(page)) = render_page(
            &self.chapter,
            &self.styles,
            &self.index,
            index,
            self.viewport,
            self.settings,
            &mut measurer,
        ) else {
            return false;
        };

        self.current_page_index = index;
        self.current_page = page.into_owned();
        true
    }

    pub fn page_at_position(&self, position: ReadingPosition) -> Option<usize> {
        self.index.page_at_position(position)
    }

    #[cfg(test)]
    pub(super) fn has_decoded_image(&self, path: &ArchivePath) -> bool {
        self.images.contains(path)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReaderDocument {
    path: String,
    identifier: Option<String>,
    title: Option<String>,
    creators: Vec<String>,
    package_path: String,
    progress: BookProgressMap,
    chapter: ReaderChapter,
    opening_page_index: usize,
}

impl ReaderDocument {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        path: String,
        identifier: Option<String>,
        title: Option<String>,
        creators: Vec<String>,
        package_path: String,
        progress: BookProgressMap,
        chapter: ReaderChapter,
        opening_page_index: usize,
    ) -> Self {
        Self {
            path,
            identifier,
            title,
            creators,
            package_path,
            progress,
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

    pub fn chapter_path(&self) -> &str {
        self.chapter.chapter_path()
    }

    pub const fn spine(&self) -> SpineIndex {
        self.chapter.spine()
    }

    pub fn page_count(&self) -> usize {
        self.chapter.page_count()
    }

    pub fn page(&self, index: usize) -> Option<&Page<'static>> {
        self.chapter.page(index)
    }

    #[cfg(test)]
    pub fn page_position(&self, index: usize) -> Option<ReadingPosition> {
        self.chapter.page_position(index)
    }

    pub(super) fn select_page(&mut self, index: usize, fonts: &ReaderFonts) -> bool {
        self.chapter.select_page(index, fonts)
    }

    pub(super) fn missing_page_images(&self) -> Vec<ArchivePath> {
        self.chapter.missing_page_images()
    }

    pub(super) fn replace_images(&mut self, images: ChapterImages) {
        self.chapter.replace_images(images);
    }

    #[cfg(test)]
    pub fn first_page(&self) -> &Page<'static> {
        self.page(0)
            .expect("reader document always contains a page")
    }

    pub const fn opening_page_index(&self) -> usize {
        self.opening_page_index
    }

    pub(super) fn replace_chapter(&mut self, chapter: ReaderChapter) {
        self.chapter = chapter;
        self.opening_page_index = 0;
    }

    #[cfg(test)]
    pub(super) fn chapter(&self) -> &ReaderChapter {
        &self.chapter
    }

    pub fn progress_at_page(&self, index: usize) -> BookProgress {
        let Some(page_end) = self.chapter.page_end_position(index) else {
            return BookProgress::ZERO;
        };

        self.progress.at(
            self.spine(),
            // progress represents content consumed through the visible page,
            // so use the page end.
            page_end.location().offset(),
            self.chapter.end_position().location().offset(),
        )
    }

    pub(crate) fn register_images<'resource>(
        &mut self,
        runtime: &mut impl ResourceRuntimeApi<'resource>,
        max_slots: usize,
        registered: &mut Vec<ImageId>,
    ) {
        self.chapter.register_images(runtime, max_slots, registered);
    }

    pub fn image_source(&self, path: &ArchivePath) -> Option<ImageSource> {
        self.chapter.image_source(path)
    }
}
