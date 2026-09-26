use alloc::{format, string::String};
use inkpaper_epub::SpineIndex;
use inkpaper_reader::{Page, ReadingPosition};

use crate::{
    ReaderChapter, ReaderDocument, ReaderPreferences, ReaderPreferencesRequest,
    ReadingHistoryEntry,
    reader::{
        READER_FONT_SIZE_DEFAULT, READER_FONT_SIZE_MAX, READER_FONT_SIZE_MIN, READER_FONT_SIZE_STEP,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReaderChapterDirection {
    Previous,
    Next,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ReaderRequest {
    OpenEpub {
        path: String,
        font_size: u16,
    },

    LoadAdjacentChapter {
        path: String,
        from: SpineIndex,
        direction: ReaderChapterDirection,
    },

    RepaginateChapter {
        path: String,
        spine: SpineIndex,
        font_size: u16,
    },

    JumpTo {
        path: String,
        spine: SpineIndex,
        anchor: Option<String>,
    },

    UpdateProgress(ReadingHistoryEntry),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PendingRepaginationRequest {
    spine: SpineIndex,
    position: ReadingPosition,
    font_size: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingJumpRequest {
    spine: SpineIndex,
    anchor: Option<String>,
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

#[derive(Debug)]
pub(crate) struct ReaderState {
    path: String,
    fallback_title: String,

    pending: Option<ReaderRequest>,
    pending_preferences: Option<ReaderPreferencesRequest>,
    pending_chapter: Option<PendingChapterRequest>,
    pending_repagination: Option<PendingRepaginationRequest>,
    pending_jump: Option<PendingJumpRequest>,

    document: Option<ReaderDocument>,
    page_index: usize,
    font_size: u16,

    failed: bool,
    chrome: ReaderChromeState,
}

impl Default for ReaderState {
    fn default() -> Self {
        Self {
            path: String::new(),
            fallback_title: String::new(),
            pending: None,
            pending_preferences: Some(ReaderPreferencesRequest::Load),
            pending_chapter: None,
            pending_repagination: None,
            pending_jump: None,
            document: None,
            page_index: 0,
            font_size: READER_FONT_SIZE_DEFAULT,
            failed: false,
            chrome: ReaderChromeState::default(),
        }
    }
}

impl ReaderState {
    pub(crate) fn open(&mut self, path: String, fallback_title: String) {
        self.pending = Some(ReaderRequest::OpenEpub {
            path: path.clone(),
            font_size: self.font_size,
        });

        self.pending_chapter = None;
        self.pending_repagination = None;
        self.pending_jump = None;

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

    pub(crate) fn take_preferences_request(&mut self) -> Option<ReaderPreferencesRequest> {
        self.pending_preferences.take()
    }

    pub(crate) fn apply_preferences(&mut self, preferences: ReaderPreferences) -> bool {
        let font_size = preferences.font_size();

        if font_size == self.font_size {
            return false;
        }

        if self.document.is_some() {
            return self.request_font_size(font_size);
        }

        self.font_size = font_size;

        true
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
        self.pending_repagination = None;
        self.pending_jump = None;
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
        self.pending_repagination = None;
        self.pending_jump = None;
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
        if self.pending_repagination.is_some() || self.pending_jump.is_some() {
            return false;
        }

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
        if self.pending_repagination.is_some() || self.pending_jump.is_some() {
            return false;
        }

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
        if self.pending_chapter.is_some()
            || self.pending_repagination.is_some()
            || self.pending_jump.is_some()
        {
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

    /// Requests the chapter at `spine`, opened on the page holding `anchor`.
    // called by the table of contents screen
    #[allow(dead_code)]
    pub(crate) fn jump_to(&mut self, spine: SpineIndex, anchor: Option<String>) -> bool {
        if self.document.is_none()
            || self.pending_chapter.is_some()
            || self.pending_repagination.is_some()
            || self.pending_jump.is_some()
        {
            return false;
        }

        self.pending_jump = Some(PendingJumpRequest {
            spine,
            anchor: anchor.clone(),
        });

        self.pending = Some(ReaderRequest::JumpTo {
            path: self.path.clone(),
            spine,
            anchor,
        });

        true
    }

    pub(crate) fn apply_jump(
        &mut self,
        path: &str,
        spine: SpineIndex,
        anchor: Option<String>,
        chapter: ReaderChapter,
        page_index: usize,
    ) -> bool {
        if path != self.path || chapter.spine() != spine {
            return false;
        }

        if self.pending_jump != Some(PendingJumpRequest { spine, anchor }) {
            return false;
        }

        self.pending_jump = None;

        let Some(document) = self.document.as_mut() else {
            return false;
        };

        document.replace_chapter(chapter);

        self.page_index = page_index.min(document.page_count().saturating_sub(1));
        self.failed = false;

        self.chrome.controls_visible = false;

        self.refresh_chrome();
        self.queue_progress_update();

        true
    }

    pub(crate) fn finish_jump_request(
        &mut self,
        path: &str,
        spine: SpineIndex,
        anchor: Option<String>,
    ) -> bool {
        if path != self.path || self.pending_jump != Some(PendingJumpRequest { spine, anchor }) {
            return false;
        }

        self.pending_jump = None;

        true
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

        if document.page(self.page_index).is_none() {
            return;
        }

        let page_count = document.page_count();

        if page_count == 0 {
            return;
        }

        let page_number = self.page_index.saturating_add(1).min(page_count);

        let book_progress = document.progress_at_page(self.page_index);

        let section_number = document.spine().get().saturating_add(1);

        self.chrome.page_label = format!("{} / {}", page_number, page_count,);

        self.chrome.section_label = format!(
            "{}%  S {} / {}",
            book_progress.percent(),
            section_number,
            document.spine_len(),
        );
    }

    pub(crate) fn current_progress(&self) -> Option<ReadingHistoryEntry> {
        let document = self.document.as_ref()?;

        let page = document.page(self.page_index)?;

        let title = document
            .title()
            .filter(|title| !title.trim().is_empty())
            .unwrap_or(&self.fallback_title);

        let creator = document
            .creators()
            .iter()
            .find(|creator| !creator.trim().is_empty())
            .cloned();

        Some(ReadingHistoryEntry::new(
            self.path.clone(),
            document.identifier().map(String::from),
            String::from(title),
            creator,
            page.position(),
            document.progress_at_page(self.page_index),
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

    pub(crate) fn document(&self) -> Option<&ReaderDocument> {
        self.document.as_ref()
    }

    pub(crate) fn decrease_font_size(&mut self) -> bool {
        let target = self
            .font_size
            .saturating_sub(READER_FONT_SIZE_STEP)
            .max(READER_FONT_SIZE_MIN);

        self.request_font_size(target)
    }

    pub(crate) fn increase_font_size(&mut self) -> bool {
        let target = self
            .font_size
            .saturating_add(READER_FONT_SIZE_STEP)
            .min(READER_FONT_SIZE_MAX);

        self.request_font_size(target)
    }

    fn request_font_size(&mut self, font_size: u16) -> bool {
        if font_size == self.font_size
            || self.pending_chapter.is_some()
            || self.pending_repagination.is_some()
            || self.pending_jump.is_some()
        {
            return false;
        }

        let Some(document) = self.document.as_ref() else {
            return false;
        };

        let Some(page) = document.page(self.page_index) else {
            return false;
        };

        let request = PendingRepaginationRequest {
            spine: document.spine(),
            position: page.position(),
            font_size,
        };

        self.pending_repagination = Some(request);

        self.pending = Some(ReaderRequest::RepaginateChapter {
            path: self.path.clone(),
            spine: request.spine,
            font_size,
        });

        true
    }

    pub(crate) fn apply_repaginated_chapter(
        &mut self,
        path: &str,
        spine: SpineIndex,
        font_size: u16,
        chapter: ReaderChapter,
    ) -> bool {
        if path != self.path {
            return false;
        }

        let Some(request) = self.pending_repagination else {
            return false;
        };

        if request.spine != spine || request.font_size != font_size || chapter.spine() != spine {
            return false;
        }

        let Some(document) = self.document.as_mut() else {
            self.pending_repagination = None;
            return false;
        };

        if document.spine() != spine {
            self.pending_repagination = None;
            return false;
        }

        let page_index = chapter.page_at_position(request.position).unwrap_or(0);

        document.replace_chapter(chapter);

        self.page_index = page_index.min(document.page_count().saturating_sub(1));

        self.font_size = font_size;
        self.pending_repagination = None;
        self.failed = false;

        // keep the chrome visible so repeated A-/A+ presses are convenient.
        self.chrome.controls_visible = true;

        self.refresh_chrome();
        self.queue_preferences_update();
        self.queue_progress_update();

        true
    }

    pub(crate) fn finish_repagination_request(
        &mut self,
        path: &str,
        spine: SpineIndex,
        font_size: u16,
    ) -> bool {
        if path != self.path {
            return false;
        }

        let Some(request) = self.pending_repagination else {
            return false;
        };

        if request.spine != spine || request.font_size != font_size {
            return false;
        }

        self.pending_repagination = None;

        true
    }

    pub(crate) const fn font_size(&self) -> u16 {
        self.font_size
    }

    fn queue_preferences_update(&mut self) {
        let preferences = ReaderPreferences::new(self.font_size)
            .expect("reader state only contains valid font sizes");

        self.pending_preferences = Some(ReaderPreferencesRequest::Update(preferences));
    }
}
