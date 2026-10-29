use alloc::string::String;
use inkpaper_epub::SpineIndex;

use crate::ReaderChapter;

use super::{
    JumpTarget, PendingChapterRequest, ReaderChapterDirection, ReaderRequest, ReaderState,
};

impl ReaderState {
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

        self.chrome.menu_open = false;

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

            self.chrome.menu_open = false;

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

            self.chrome.menu_open = false;

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
    pub(crate) fn jump_to(&mut self, spine: SpineIndex, anchor: Option<String>) -> bool {
        self.jump(JumpTarget::Chapter { spine, anchor })
    }

    pub(super) fn jump(&mut self, target: JumpTarget) -> bool {
        if self.document.is_none()
            || self.pending_chapter.is_some()
            || self.pending_repagination.is_some()
            || self.pending_jump.is_some()
        {
            return false;
        }

        self.pending_jump = Some(target.clone());

        self.pending = Some(ReaderRequest::JumpTo {
            path: self.path.clone(),
            target,
        });

        true
    }

    pub(crate) fn apply_jump(
        &mut self,
        path: &str,
        target: JumpTarget,
        chapter: ReaderChapter,
        page_index: usize,
    ) -> bool {
        if path != self.path || self.pending_jump.as_ref() != Some(&target) {
            return false;
        }

        self.pending_jump = None;

        let Some(document) = self.document.as_mut() else {
            return false;
        };

        document.replace_chapter(chapter);

        self.page_index = page_index.min(document.page_count().saturating_sub(1));
        self.failed = false;

        self.chrome.menu_open = false;

        self.refresh_chrome();
        self.queue_progress_update();

        true
    }

    pub(crate) fn finish_jump_request(&mut self, path: &str, target: JumpTarget) -> bool {
        if path != self.path || self.pending_jump.as_ref() != Some(&target) {
            return false;
        }

        self.pending_jump = None;

        true
    }
}
