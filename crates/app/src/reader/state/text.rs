use inkpaper_epub::SpineIndex;

use crate::{ReaderChapter, ReaderPreferences, ReaderPreferencesRequest, reader::TextSettings};

use super::{PendingRepaginationRequest, ReaderRequest, ReaderState};

impl ReaderState {
    /// Repaginates the current chapter with `text`, keeping the reading position.
    pub(crate) fn request_text_settings(&mut self, text: TextSettings) -> bool {
        if text == self.text
            || self.pending_chapter.is_some()
            || self.pending_repagination.is_some()
            || self.pending_jump.is_some()
        {
            return false;
        }

        // the page only paints differently
        if text.same_layout(self.text) {
            self.text = text;
            return true;
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
            text,
        };

        self.pending_repagination = Some(request);

        self.pending = Some(ReaderRequest::RepaginateChapter {
            path: self.path.clone(),
            spine: request.spine,
            text,
        });

        true
    }

    pub(crate) fn apply_repaginated_chapter(
        &mut self,
        path: &str,
        spine: SpineIndex,
        text: TextSettings,
        chapter: ReaderChapter,
    ) -> bool {
        if path != self.path {
            return false;
        }

        let Some(request) = self.pending_repagination else {
            return false;
        };

        if request.spine != spine || request.text != text || chapter.spine() != spine {
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

        self.text = text;
        self.saved_text = text;
        self.pending_repagination = None;
        self.failed = false;

        self.refresh_chrome();
        self.queue_preferences_update();
        self.queue_progress_update();

        true
    }

    pub(crate) fn finish_repagination_request(
        &mut self,
        path: &str,
        spine: SpineIndex,
        text: TextSettings,
    ) -> bool {
        if path != self.path {
            return false;
        }

        let Some(request) = self.pending_repagination else {
            return false;
        };

        if request.spine != spine || request.text != text {
            return false;
        }

        self.pending_repagination = None;

        true
    }

    /// The settings the page is laid out with.
    pub(crate) const fn text_settings(&self) -> TextSettings {
        self.text
    }

    /// Keeps `text` from the Text Settings screen. It is saved right away; an
    /// open book is laid out with it once the screen closes.
    pub(crate) fn save_text_settings(&mut self, text: TextSettings) {
        if self.document.is_none() {
            self.text = text;
        }

        self.saved_text = text;
        self.queue_preferences_update();
    }

    pub(super) fn queue_preferences_update(&mut self) {
        let preferences = ReaderPreferences::new(self.saved_text).with_dictionary(self.dictionary);

        self.pending_preferences = Some(ReaderPreferencesRequest::Update(preferences));
    }
}
