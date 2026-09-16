use alloc::{format, string::String};
use inkpaper_epub::SpineIndex;
use inkpaper_reader::{Page, ReadingPosition};

use crate::{ReaderChapter, ReaderDocument, ReadingProgress};

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

    #[cfg(test)]
    pub(super) const fn page_index(&self) -> usize {
        self.page_index
    }

    #[cfg(test)]
    pub(super) fn document(&self) -> Option<&ReaderDocument> {
        self.document.as_ref()
    }
}
