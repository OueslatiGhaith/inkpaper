use alloc::boxed::Box;
use inkpaper_reader::{Page, PageItem, Pagination, ReadingPosition, SpineIndex, Viewport};

use super::ReaderPageResources;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ChapterDirection {
    Previous,
    Next,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChapterRequest {
    pub from: SpineIndex,
    pub direction: ChapterDirection,
}

#[cfg(feature = "defmt")]
impl defmt::Format for ChapterRequest {
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(
            fmt,
            "ChapterRequest {{ from: {}, direction: {} }}",
            self.from.get(),
            self.direction
        )
    }
}

/// page indices are local to the current pagination, not persisted book positions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageRequest {
    pub spine: SpineIndex,
    pub from: usize,
    pub to: usize,
}

#[cfg(feature = "defmt")]
impl defmt::Format for PageRequest {
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(
            fmt,
            "PageRequest {{ spine: {}, from: {}, to: {} }}",
            self.spine.get(),
            self.from,
            self.to
        );
    }
}

pub enum ChapterLoadOutcome {
    Ready(ReaderSession),
    Boundary,
    Failed,
}

pub enum PageLoadOutcome {
    Ready(Box<dyn ReaderPageResources>),
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ReaderNotice {
    BeginningOfBook,
    EndOfBook,
    ChapterLoadFailed,
    PageLoadFailed,
}

impl ReaderNotice {
    pub const fn message(self) -> &'static str {
        match self {
            Self::BeginningOfBook => "Beginning of book",
            Self::EndOfBook => "End of book",
            Self::ChapterLoadFailed => "Could not load chapter. Try again.",
            Self::PageLoadFailed => "Could not load page. Try again.",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PendingRequest {
    Chapter(ChapterDirection),
    Page(PageRequest),
}

pub struct ReaderSession {
    pagination: Pagination<'static>,
    viewport: Viewport,
    resources: Box<dyn ReaderPageResources>,
    spine: SpineIndex,
    page_index: usize,
    requested: Option<PendingRequest>,
    request_sent: bool,
    notice: Option<ReaderNotice>,
}

impl ReaderSession {
    pub fn new(
        pagination: Pagination<'static>,
        viewport: Viewport,
        resources: Box<dyn ReaderPageResources>,
    ) -> Option<Self> {
        let spine = pagination.pages().first()?.start().spine();
        Some(Self {
            pagination,
            viewport,
            resources,
            spine,
            page_index: 0,
            requested: None,
            request_sent: false,
            notice: None,
        })
    }

    pub const fn viewport(&self) -> Viewport {
        self.viewport
    }

    pub const fn spine(&self) -> SpineIndex {
        self.spine
    }

    pub fn resources(&self) -> &dyn ReaderPageResources {
        self.resources.as_ref()
    }

    pub const fn page_index(&self) -> usize {
        self.page_index
    }

    pub fn page_number(&self) -> usize {
        self.page_index.saturating_add(1)
    }

    pub fn page_count(&self) -> usize {
        self.pagination.len()
    }

    pub const fn notice(&self) -> Option<ReaderNotice> {
        self.notice
    }

    pub(crate) fn clear_notice(&mut self) {
        self.notice = None;
    }

    pub fn current_page(&self) -> &Page<'static> {
        self.pagination
            .pages()
            .get(self.page_index)
            .expect("reader page index must remain inside pagination")
    }

    pub fn previous_page(&mut self) -> bool {
        if self.requested.is_some() {
            return false;
        }
        if self.page_index == 0 {
            self.requested = Some(PendingRequest::Chapter(ChapterDirection::Previous));
            return false;
        }

        self.turn_to(self.page_index - 1)
    }

    pub fn next_page(&mut self) -> bool {
        if self.requested.is_some() {
            return false;
        }
        if self.page_index + 1 == self.pagination.len() {
            self.requested = Some(PendingRequest::Chapter(ChapterDirection::Next));
            return false;
        }

        self.turn_to(self.page_index + 1)
    }

    fn turn_to(&mut self, to: usize) -> bool {
        let page = &self.pagination.pages()[to];
        let needs_images = page.items().iter().any(|item| match item {
            PageItem::Image(fragment) => self.resources.image_source(fragment.image()).is_none(),
            PageItem::Text(_) => false,
        });

        if needs_images {
            self.requested = Some(PendingRequest::Page(PageRequest {
                spine: self.spine,
                from: self.page_index,
                to,
            }));

            return false;
        }

        self.page_index = to;
        self.clear_notice();

        true
    }

    pub(crate) fn take_page_request(&mut self) -> Option<PageRequest> {
        if self.request_sent {
            return None;
        }
        let PendingRequest::Page(request) = self.requested? else {
            return None;
        };

        self.request_sent = true;

        Some(request)
    }

    pub fn page_for_request(&self, request: PageRequest) -> Option<&Page<'static>> {
        if !self.request_sent
            || self.requested != Some(PendingRequest::Page(request))
            || request.spine != self.spine
            || request.from != self.page_index
        {
            return None;
        }

        self.pagination.pages().get(request.to)
    }

    pub(crate) fn complete_page_request(
        &mut self,
        request: PageRequest,
        outcome: PageLoadOutcome,
    ) -> bool {
        let Some(page) = self.page_for_request(request) else {
            return false;
        };
        let PageLoadOutcome::Ready(resources) = outcome else {
            self.cancel_request();
            self.notice = Some(ReaderNotice::PageLoadFailed);
            return false;
        };

        let images_ready = page.items().iter().all(|item| match item {
            PageItem::Image(fragment) => resources.image_source(fragment.image()).is_some(),
            PageItem::Text(_) => true,
        });
        let fonts_unchanged = self
            .pagination
            .pages()
            .iter()
            .flat_map(|page| page.items())
            .all(|item| match item {
                PageItem::Text(fragment) => {
                    resources.font_for(fragment.style())
                        == self.resources.font_for(fragment.style())
                }
                PageItem::Image(_) => true,
            });

        self.cancel_request();

        if !images_ready || !fonts_unchanged {
            self.notice = Some(ReaderNotice::PageLoadFailed);
            return false;
        }

        self.resources = resources;
        self.page_index = request.to;
        self.clear_notice();

        true
    }

    pub(crate) fn take_chapter_request(&mut self) -> Option<ChapterRequest> {
        if self.request_sent {
            return None;
        }

        let PendingRequest::Chapter(direction) = self.requested? else {
            return None;
        };

        self.request_sent = true;

        Some(ChapterRequest {
            from: self.spine,
            direction,
        })
    }

    pub(crate) fn cancel_request(&mut self) {
        self.requested = None;
        self.request_sent = false;
    }

    pub(crate) fn complete_chapter_request(
        &mut self,
        request: ChapterRequest,
        outcome: ChapterLoadOutcome,
    ) -> bool {
        if !self.request_sent
            || request.from != self.spine
            || self.requested != Some(PendingRequest::Chapter(request.direction))
        {
            return false;
        }

        self.cancel_request();

        let mut replacement = match outcome {
            ChapterLoadOutcome::Ready(replacement) => replacement,
            ChapterLoadOutcome::Boundary => {
                self.notice = Some(match request.direction {
                    ChapterDirection::Previous => ReaderNotice::BeginningOfBook,
                    ChapterDirection::Next => ReaderNotice::EndOfBook,
                });
                return false;
            }
            ChapterLoadOutcome::Failed => {
                self.notice = Some(ReaderNotice::ChapterLoadFailed);
                return false;
            }
        };

        let valid = match request.direction {
            ChapterDirection::Previous => replacement.spine < self.spine,
            ChapterDirection::Next => replacement.spine > self.spine,
        };

        if !valid || replacement.viewport != self.viewport {
            self.notice = Some(ReaderNotice::ChapterLoadFailed);
            return false;
        }

        replacement.page_index = match request.direction {
            ChapterDirection::Previous => replacement.page_count() - 1,
            ChapterDirection::Next => 0,
        };

        if replacement
            .current_page()
            .items()
            .iter()
            .any(|item| match item {
                PageItem::Image(fragment) => replacement
                    .resources
                    .image_source(fragment.image())
                    .is_none(),
                PageItem::Text(_) => false,
            })
        {
            self.notice = Some(ReaderNotice::ChapterLoadFailed);
            return false;
        }

        replacement.cancel_request();
        replacement.clear_notice();
        *self = replacement;

        true
    }

    /// opens an already prepared page containing the saved position
    ///
    /// all images on that page must have resources before the session is installed
    pub fn at_position(
        pagination: Pagination<'static>,
        viewport: Viewport,
        resources: Box<dyn ReaderPageResources>,
        position: ReadingPosition,
    ) -> Option<Self> {
        let page_index = pagination.page_at_position(position)?;
        let page = &pagination.pages()[page_index];

        if page.items().iter().any(|item| match item {
            PageItem::Image(fragment) => resources.image_source(fragment.image()).is_none(),
            PageItem::Text(_) => false,
        }) {
            return None;
        }

        let mut session = Self::new(pagination, viewport, resources)?;
        session.page_index = page_index;

        Some(session)
    }
}
