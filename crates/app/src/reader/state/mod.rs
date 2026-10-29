mod links;
mod menu;
mod navigation;
mod selection;
mod text;
mod toc;

use alloc::{format, string::String, vec::Vec};
use inkpaper_epub::{ArchivePath, SpineIndex};
use inkpaper_reader::{Page, ReadingPosition};

use crate::{
    ReaderDocument, ReaderPreferences, ReaderPreferencesRequest, ReadingHistoryEntry,
    paging::Pager,
    reader::{TableOfContents, TextSetting, TextSettings, WordSelection},
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
        target: JumpTarget,
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

/// Where a jump lands. The service finds the chapter and the page in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum JumpTarget {
    /// a chapter, at an anchor or its start, like a chapter list entry
    Chapter {
        spine: SpineIndex,
        anchor: Option<String>,
    },
    /// a link's destination: the resource it names, at an anchor or its start
    Link {
        resource: ArchivePath,
        anchor: Option<String>,
    },
    /// a saved reading position, to go back after following a link
    Position(ReadingPosition),
}

/// Like crosspoint, following a link saves where it was followed from, up to
/// this many times; a deeper link drops the oldest.
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
    /// whether the Links and footnotes picker is open
    link_picker: bool,
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
    pending_jump: Option<JumpTarget>,
    /// positions links were followed from, the latest last
    link_returns: Vec<ReadingPosition>,

    document: Option<ReaderDocument>,
    page_index: usize,
    /// the settings the page is laid out with
    text: TextSettings,
    /// the settings last saved; an open book is laid out with them once it
    /// repaginates
    saved_text: TextSettings,
    /// the dictionary words are looked up in
    dictionary: Option<u8>,
    /// the word held to look up, on the page it was held on
    selection: Option<WordSelection>,

    failed: bool,
    chrome: ReaderChromeState,
    toc: TableOfContents,
    toc_pager: Pager,
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
            link_returns: Vec::new(),
            document: None,
            page_index: 0,
            text: TextSettings::default(),
            saved_text: TextSettings::default(),
            dictionary: None,
            selection: None,
            failed: false,
            chrome: ReaderChromeState::default(),
            toc: TableOfContents::NotLoaded,
            toc_pager: Pager::default(),
        }
    }
}

impl ReaderState {
    pub(crate) fn open(&mut self, path: String, fallback_title: String) {
        self.text = self.saved_text;
        self.pending = Some(ReaderRequest::OpenEpub {
            path: path.clone(),
            text: self.text,
        });

        self.pending_chapter = None;
        self.pending_repagination = None;
        self.pending_jump = None;
        self.link_returns.clear();

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
        self.saved_text = text;

        let dictionary_changed = preferences.dictionary() != self.dictionary;
        self.dictionary = preferences.dictionary();

        if text == self.text {
            return dictionary_changed;
        }

        if self.document.is_some() {
            return self.request_text_settings(text) || dictionary_changed;
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

    #[cfg(test)]
    pub(super) const fn font_size(&self) -> u16 {
        self.text.font_size()
    }
}
