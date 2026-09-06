use alloc::boxed::Box;
use inkpaper_reader::{Page, Pagination, SpineIndex, Viewport};

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

pub struct ReaderSession {
    pagination: Pagination<'static>,
    viewport: Viewport,
    resources: Box<dyn ReaderPageResources>,
    spine: SpineIndex,
    page_index: usize,
    requested: Option<ChapterDirection>,
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
            self.requested = Some(ChapterDirection::Previous);
            return false;
        }

        self.page_index -= 1;

        true
    }

    pub fn next_page(&mut self) -> bool {
        if self.requested.is_some() {
            return false;
        }
        if self.page_index + 1 == self.pagination.len() {
            self.requested = Some(ChapterDirection::Next);
            return false;
        }

        self.page_index += 1;

        true
    }

    pub(crate) fn take_chapter_request(&mut self) -> Option<ChapterRequest> {
        if self.request_sent {
            return None;
        }

        let direction = self.requested?;
        self.request_sent = true;

        Some(ChapterRequest {
            from: self.spine,
            direction,
        })
    }

    pub(crate) fn cancel_chapter_request(&mut self) {
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
            || self.requested != Some(request.direction)
        {
            return false;
        }

        self.cancel_chapter_request();

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
