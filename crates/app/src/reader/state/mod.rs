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
    ReaderDocument, ReaderPreferences, ReadingHistoryEntry,
    fonts::{CardFont, card_for},
    paging::Pager,
    reader::{
        ChapterImages, TableOfContents, TextSettings, WordSelection, measurer::CachedReaderFonts,
    },
    resume::ResumeLimit,
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
        /// reopened at boot, where nothing on screen asked for it
        resume: bool,
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

    LoadPageImages {
        path: String,
        spine: SpineIndex,
        page_index: usize,
        images: Vec<ArchivePath>,
    },

    UpdateProgress(ReadingHistoryEntry),
}

impl ReaderRequest {
    /// Whether the request takes long enough that the screen should first show
    /// what asked for it, like the toast of a book opening. A book reopened at
    /// boot has nothing to show first, so the first frame is its page.
    pub(crate) const fn takes_long(&self) -> bool {
        matches!(self, Self::OpenEpub { resume: false, .. })
    }
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

#[derive(Debug, Default)]
struct ReaderChromeState {
    menu_open: bool,
    page_label: String,
    progress_label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingPageImagesRequest {
    spine: SpineIndex,
    page_index: usize,
    images: Vec<ArchivePath>,
}

#[derive(Debug)]
pub(crate) struct ReaderState {
    path: String,
    fallback_title: String,

    pending_document: Option<ReaderRequest>,
    pending_toc: Option<String>,
    pending_page_images: Option<PendingPageImagesRequest>,
    pending_progress: Option<ReadingHistoryEntry>,
    pending_preferences: Option<ReaderPreferences>,
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
    /// how long after sleep the book read then opens again at boot
    resume_limit: ResumeLimit,
    /// the word held to look up, on the page it was held on
    selection: Option<WordSelection>,
    /// fonts cached across word-selection presses and drags
    selection_fonts: CachedReaderFonts,

    chrome: ReaderChromeState,
    toc: TableOfContents,
    toc_pager: Pager,
}

impl Default for ReaderState {
    fn default() -> Self {
        Self {
            path: String::new(),
            fallback_title: String::new(),
            pending_document: None,
            pending_toc: None,
            pending_page_images: None,
            pending_progress: None,
            pending_preferences: None,
            pending_chapter: None,
            pending_repagination: None,
            pending_jump: None,
            link_returns: Vec::new(),
            document: None,
            page_index: 0,
            text: TextSettings::default(),
            saved_text: TextSettings::default(),
            dictionary: None,
            resume_limit: ResumeLimit::Always,
            selection: None,
            selection_fonts: CachedReaderFonts::default(),
            chrome: ReaderChromeState::default(),
            toc: TableOfContents::NotLoaded,
            toc_pager: Pager::default(),
        }
    }
}

impl ReaderState {
    pub(crate) fn open(&mut self, path: String, fallback_title: String) {
        self.start_opening(path, fallback_title, false);
    }

    /// Reopens the book read when the device went to sleep.
    pub(crate) fn resume(&mut self, path: String, fallback_title: String) {
        self.start_opening(path, fallback_title, true);
    }

    fn start_opening(&mut self, path: String, fallback_title: String, resume: bool) {
        self.text = self.saved_text;
        self.pending_document = Some(ReaderRequest::OpenEpub {
            path: path.clone(),
            text: self.text,
            resume,
        });
        self.pending_toc = None;
        self.pending_page_images = None;

        self.pending_chapter = None;
        self.pending_repagination = None;
        self.pending_jump = None;
        self.link_returns.clear();

        self.path = path;
        self.fallback_title = fallback_title;
        self.document = None;
        self.page_index = 0;
        self.chrome = ReaderChromeState::default();
        self.toc = TableOfContents::NotLoaded;
    }

    pub(super) const fn has_pending_navigation(&self) -> bool {
        self.pending_chapter.is_some()
            || self.pending_repagination.is_some()
            || self.pending_jump.is_some()
    }

    pub(crate) fn take_request(&mut self) -> Option<ReaderRequest> {
        if let Some(progress) = self.pending_progress.take() {
            return Some(ReaderRequest::UpdateProgress(progress));
        }

        if let Some(request) = self.pending_document.take() {
            return Some(request);
        }

        if let Some(request) = self.pending_page_images.take() {
            return Some(ReaderRequest::LoadPageImages {
                path: self.path.clone(),
                spine: request.spine,
                page_index: request.page_index,
                images: request.images,
            });
        }

        self.pending_toc
            .take()
            .map(|path| ReaderRequest::LoadTableOfContents { path })
    }

    pub(crate) const fn resume_limit(&self) -> ResumeLimit {
        self.resume_limit
    }

    /// Returns whether the limit changed.
    pub(crate) fn choose_resume_limit(&mut self, limit: ResumeLimit) -> bool {
        if limit == self.resume_limit {
            return false;
        }

        self.resume_limit = limit;
        self.queue_preferences_update();

        true
    }

    /// Puts back a request the service took, unless a newer one is waiting.
    pub(crate) fn restore_request(&mut self, request: ReaderRequest) {
        match request {
            ReaderRequest::UpdateProgress(progress) => {
                self.pending_progress.get_or_insert(progress);
            }
            ReaderRequest::LoadTableOfContents { path } => {
                if self.path == path && self.toc == TableOfContents::Loading {
                    self.pending_toc.get_or_insert(path);
                }
            }
            ReaderRequest::LoadPageImages {
                path,
                spine,
                page_index,
                images,
            } => {
                if self.path == path
                    && self.page_index == page_index
                    && self
                        .document
                        .as_ref()
                        .is_some_and(|document| document.spine() == spine)
                {
                    self.pending_page_images
                        .get_or_insert(PendingPageImagesRequest {
                            spine,
                            page_index,
                            images,
                        });
                }
            }
            document_request => {
                self.pending_document.get_or_insert(document_request);
            }
        }
    }

    pub(crate) fn take_preferences_request(&mut self) -> Option<ReaderPreferences> {
        self.pending_preferences.take()
    }

    pub(crate) fn apply_preferences(&mut self, preferences: ReaderPreferences) -> bool {
        let text = preferences.text();
        self.saved_text = text;

        let settings_changed = preferences.dictionary() != self.dictionary
            || preferences.resume_limit() != self.resume_limit;
        self.dictionary = preferences.dictionary();
        self.resume_limit = preferences.resume_limit();

        if text == self.text {
            return settings_changed;
        }

        if self.document.is_some() {
            return self.request_text_settings(text) || settings_changed;
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

        self.pending_document = None;
        self.pending_page_images = None;
        self.pending_chapter = None;
        self.pending_repagination = None;
        self.pending_jump = None;
        self.page_index = page_index;

        self.chrome.menu_open = false;

        self.queue_missing_page_images();
        self.refresh_chrome();
        self.queue_progress_update();

        true
    }

    pub(crate) fn apply_page_images(
        &mut self,
        path: &str,
        spine: SpineIndex,
        page_index: usize,
        images: ChapterImages,
    ) -> bool {
        if path != self.path || self.page_index != page_index {
            return false;
        }

        let Some(document) = self.document.as_mut() else {
            return false;
        };

        if document.spine() != spine {
            return false;
        }

        document.replace_images(images);
        true
    }

    pub(super) fn select_page(&mut self, index: usize, card: Option<&CardFont>) -> bool {
        let Ok(fonts) = self
            .selection_fonts
            .get_or_init(card_for(self.text.font(), card))
        else {
            return false;
        };

        let Some(document) = self.document.as_mut() else {
            return false;
        };

        if !document.select_page(index, fonts) {
            return false;
        }

        self.page_index = index;
        self.queue_missing_page_images();
        true
    }

    pub(super) fn queue_missing_page_images(&mut self) {
        self.pending_page_images = None;

        let Some(document) = self.document.as_ref() else {
            return;
        };

        let missing = document.missing_page_images();
        if !missing.is_empty() {
            self.pending_page_images = Some(PendingPageImagesRequest {
                spine: document.spine(),
                page_index: self.page_index,
                images: missing,
            });
        }
    }

    pub(crate) fn apply_error(&mut self, path: &str) -> bool {
        if path != self.path {
            return false;
        }

        self.document = None;
        self.pending_document = None;
        self.pending_toc = None;
        self.pending_page_images = None;
        self.pending_chapter = None;
        self.pending_repagination = None;
        self.pending_jump = None;
        self.page_index = 0;

        self.chrome = ReaderChromeState::default();
        self.toc = TableOfContents::NotLoaded;

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

        self.pending_progress = Some(progress);
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
