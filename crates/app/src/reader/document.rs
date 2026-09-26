use alloc::{string::String, vec::Vec};

use inkpaper_epub::{ArchivePath, SpineIndex};
use inkpaper_reader::{Page, Pagination, ReadingPosition};
use inkpaper_ui::{ImageSource, ResourceRuntimeApi};

use crate::{
    BookProgress,
    reader::{images::ChapterImages, progress::BookProgressMap},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReaderChapter {
    chapter_path: String,
    spine: SpineIndex,
    pagination: Pagination<'static>,
    images: ChapterImages,
}

impl ReaderChapter {
    pub(super) fn new(
        chapter_path: String,
        spine: SpineIndex,
        pagination: Pagination<'static>,
    ) -> Self {
        Self::with_images(chapter_path, spine, pagination, ChapterImages::default())
    }

    pub(super) fn with_images(
        chapter_path: String,
        spine: SpineIndex,
        pagination: Pagination<'static>,
        images: ChapterImages,
    ) -> Self {
        Self {
            chapter_path,
            spine,
            pagination,
            images,
        }
    }

    pub fn register_images<'resource>(&mut self, runtime: &mut impl ResourceRuntimeApi<'resource>) {
        self.images.register(runtime);
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
        self.pagination.len()
    }

    pub fn page(&self, index: usize) -> Option<&Page<'static>> {
        self.pagination.pages().get(index)
    }

    pub fn page_at_position(&self, position: ReadingPosition) -> Option<usize> {
        self.pagination.page_at_position(position)
    }

    #[cfg(test)]
    pub(super) fn pagination(&self) -> &Pagination<'static> {
        &self.pagination
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

    pub fn package_path(&self) -> &str {
        &self.package_path
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
        let Some(page) = self.page(index) else {
            return BookProgress::ZERO;
        };

        let Some(chapter_end) = self.page(self.page_count().saturating_sub(1)) else {
            return BookProgress::ZERO;
        };

        self.progress.at(
            self.spine(),
            // progress represents content consumed through the visible page,
            // so use the page end.
            page.end_position().location().offset(),
            chapter_end.end_position().location().offset(),
        )
    }

    pub fn register_images<'resource>(&mut self, runtime: &mut impl ResourceRuntimeApi<'resource>) {
        self.chapter.register_images(runtime);
    }

    pub fn image_source(&self, path: &ArchivePath) -> Option<ImageSource> {
        self.chapter.image_source(path)
    }
}
