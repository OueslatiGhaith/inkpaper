use alloc::{format, string::String, vec::Vec};
use inkpaper_epub::SpineIndex;
use inkpaper_reader::{Page, ReadingPosition};

use crate::{
    ReaderChapter, ReaderDocument, ReaderPreferences, ReaderPreferencesRequest,
    ReadingHistoryEntry,
    reader::{TableOfContents, TextSetting, TextSettings, TocEntry, toc::current_toc_index},
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
        text: TextSettings,
    },

    LoadAdjacentChapter {
        path: String,
        from: SpineIndex,
        direction: ReaderChapterDirection,
    },

    RepaginateChapter {
        path: String,
        spine: SpineIndex,
        text: TextSettings,
    },

    JumpTo {
        path: String,
        spine: SpineIndex,
        anchor: Option<String>,
    },

    LoadTableOfContents {
        path: String,
    },

    UpdateProgress(ReadingHistoryEntry),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PendingRepaginationRequest {
    spine: SpineIndex,
    position: ReadingPosition,
    text: TextSettings,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ReaderMenuTab {
    #[default]
    Text,
    More,
}

#[derive(Debug, Default)]
struct ReaderChromeState {
    menu_open: bool,
    menu_tab: ReaderMenuTab,
    /// the Text panel row whose picker is open
    text_picker: Option<TextSetting>,
    page_label: String,
    progress_label: String,
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
    text: TextSettings,

    failed: bool,
    chrome: ReaderChromeState,
    toc: TableOfContents,
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
            text: TextSettings::default(),
            failed: false,
            chrome: ReaderChromeState::default(),
            toc: TableOfContents::NotLoaded,
        }
    }
}

impl ReaderState {
    pub(crate) fn open(&mut self, path: String, fallback_title: String) {
        self.pending = Some(ReaderRequest::OpenEpub {
            path: path.clone(),
            text: self.text,
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
        self.toc = TableOfContents::NotLoaded;
    }

    pub(crate) fn take_request(&mut self) -> Option<ReaderRequest> {
        self.pending.take()
    }

    pub(crate) fn take_preferences_request(&mut self) -> Option<ReaderPreferencesRequest> {
        self.pending_preferences.take()
    }

    pub(crate) fn apply_preferences(&mut self, preferences: ReaderPreferences) -> bool {
        let text = preferences.text();

        if text == self.text {
            return false;
        }

        if self.document.is_some() {
            return self.request_text_settings(text);
        }

        self.text = text;

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

        self.chrome.menu_open = false;

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

        self.chrome.menu_open = false;

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

    pub(crate) fn open_menu(&mut self) -> bool {
        if self.document.is_none() || self.chrome.menu_open {
            return false;
        }

        self.chrome.menu_open = true;
        self.chrome.menu_tab = ReaderMenuTab::Text;
        self.chrome.text_picker = None;

        true
    }

    pub(crate) fn close_menu(&mut self) -> bool {
        if !self.chrome.menu_open {
            return false;
        }

        self.chrome.menu_open = false;
        self.chrome.text_picker = None;

        true
    }

    pub(crate) fn menu_open(&self) -> bool {
        self.chrome.menu_open
    }

    pub(crate) const fn menu_tab(&self) -> ReaderMenuTab {
        self.chrome.menu_tab
    }

    pub(crate) fn select_menu_tab(&mut self, tab: ReaderMenuTab) -> bool {
        if !self.chrome.menu_open || self.chrome.menu_tab == tab {
            return false;
        }

        self.chrome.menu_tab = tab;
        self.chrome.text_picker = None;

        true
    }

    pub(crate) const fn text_picker(&self) -> Option<TextSetting> {
        self.chrome.text_picker
    }

    pub(crate) fn open_text_picker(&mut self, setting: TextSetting) -> bool {
        if !self.chrome.menu_open || self.chrome.text_picker.is_some() {
            return false;
        }

        self.chrome.text_picker = Some(setting);

        true
    }

    /// Flips a toggle row of the Text panel.
    pub(crate) fn toggle_text_setting(&mut self, setting: TextSetting) -> bool {
        if !self.chrome.menu_open {
            return false;
        }

        setting
            .toggle(self.text)
            .is_some_and(|text| self.request_text_settings(text))
    }

    pub(crate) fn close_text_picker(&mut self) -> bool {
        self.chrome.text_picker.take().is_some()
    }

    /// Applies option `index` of the open picker and closes it. The page under
    /// the drawer repaginates, so it previews the choice.
    pub(crate) fn choose_text_option(&mut self, index: usize) -> bool {
        let Some(setting) = self.chrome.text_picker.take() else {
            return false;
        };

        if let Some(text) = setting.choose(self.text, index) {
            self.request_text_settings(text);
        }

        true
    }

    pub(crate) fn table_of_contents(&self) -> &TableOfContents {
        &self.toc
    }

    /// The table of contents entry for the chapter being read.
    pub(crate) fn current_toc_index(&self) -> Option<usize> {
        let TableOfContents::Loaded(entries) = &self.toc else {
            return None;
        };

        current_toc_index(entries, self.document.as_ref()?.spine())
    }

    /// Requests the table of contents once per book; failures retry.
    pub(crate) fn request_table_of_contents(&mut self) -> bool {
        if self.document.is_none()
            || !matches!(
                self.toc,
                TableOfContents::NotLoaded | TableOfContents::Failed
            )
        {
            return false;
        }

        self.toc = TableOfContents::Loading;
        self.pending = Some(ReaderRequest::LoadTableOfContents {
            path: self.path.clone(),
        });

        true
    }

    pub(crate) fn apply_table_of_contents(&mut self, path: &str, entries: Vec<TocEntry>) -> bool {
        if path != self.path || self.toc != TableOfContents::Loading {
            return false;
        }

        self.toc = TableOfContents::Loaded(entries);

        true
    }

    pub(crate) fn apply_table_of_contents_error(&mut self, path: &str) -> bool {
        if path != self.path || self.toc != TableOfContents::Loading {
            return false;
        }

        self.toc = TableOfContents::Failed;

        true
    }

    /// The settings the Text panel shows: the ones being repaginated to, then
    /// the current ones.
    pub(crate) fn menu_text_settings(&self) -> TextSettings {
        self.pending_repagination
            .map_or(self.text, |request| request.text)
    }

    #[cfg(test)]
    pub(crate) fn reading_position(&self) -> Option<ReadingPosition> {
        self.page().map(Page::position)
    }

    pub(crate) fn page_label(&self) -> &str {
        &self.chrome.page_label
    }

    pub(crate) fn progress_label(&self) -> &str {
        &self.chrome.progress_label
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
        self.chrome.progress_label.clear();

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

        self.chrome.page_label = format!("{} / {}", page_number, page_count,);

        self.chrome.progress_label = format!("{}%", book_progress.percent());
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

    /// Repaginates the current chapter with `text`, keeping the reading position.
    pub(crate) fn request_text_settings(&mut self, text: TextSettings) -> bool {
        if text == self.text
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

    #[cfg(test)]
    pub(super) const fn font_size(&self) -> u16 {
        self.text.font_size()
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

        let preferences = ReaderPreferences::new(text);

        self.pending_preferences = Some(ReaderPreferencesRequest::Update(preferences));
    }

    fn queue_preferences_update(&mut self) {
        let preferences = ReaderPreferences::new(self.text);

        self.pending_preferences = Some(ReaderPreferencesRequest::Update(preferences));
    }
}
