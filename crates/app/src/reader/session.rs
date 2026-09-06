use alloc::boxed::Box;
use inkpaper_reader::{Page, PageItem, Pagination, SpineIndex, Viewport};

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
        resources: Option<Box<dyn ReaderPageResources>>,
    ) -> bool {
        let Some(page) = self.page_for_request(request) else {
            return false;
        };

        let valid = resources.as_ref().is_some_and(|resources| {
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

            images_ready && fonts_unchanged
        });

        self.cancel_request();

        if !valid {
            return false;
        }

        self.resources = resources.expect("validated page resources must be present");
        self.page_index = request.to;

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

    pub(crate) fn complete_chapeter_request(
        &mut self,
        request: ChapterRequest,
        replacement: Option<Self>,
    ) -> bool {
        if !self.request_sent
            || request.from != self.spine
            || self.requested != Some(PendingRequest::Chapter(request.direction))
        {
            return false;
        }

        self.cancel_request();

        let Some(mut replacement) = replacement else {
            return false;
        };

        let valid = match request.direction {
            ChapterDirection::Previous => replacement.spine < self.spine,
            ChapterDirection::Next => replacement.spine > self.spine,
        };

        if !valid || replacement.viewport != self.viewport {
            return false;
        }
        if request.direction == ChapterDirection::Previous {
            replacement.page_index = replacement.page_count() - 1;
        }

        *self = replacement;

        true
    }
}
