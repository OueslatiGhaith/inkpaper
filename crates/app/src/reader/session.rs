use inkpaper_reader::{Page, Pagination, SpineIndex, Viewport};

use super::ReaderPageResources;

pub struct ReaderSession {
    pagination: &'static Pagination<'static>,
    viewport: Viewport,
    resources: &'static dyn ReaderPageResources,
    spine: SpineIndex,
    page_index: usize,
}

impl ReaderSession {
    pub const fn new(
        pagination: &'static Pagination<'static>,
        viewport: Viewport,
        resources: &'static dyn ReaderPageResources,
        spine: SpineIndex,
    ) -> Self {
        Self {
            pagination,
            viewport,
            resources,
            spine,
            page_index: 0,
        }
    }

    pub const fn viewport(&self) -> Viewport {
        self.viewport
    }

    pub const fn spine(&self) -> SpineIndex {
        self.spine
    }

    pub const fn resources(&self) -> &'static dyn ReaderPageResources {
        self.resources
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

    pub fn current_page(&self) -> &'static Page<'static> {
        self.pagination
            .pages()
            .get(self.page_index)
            .expect("reader page index must remain inside pagination")
    }

    pub fn previous_page(&mut self) -> bool {
        if self.page_index == 0 {
            return false;
        }

        self.page_index -= 1;

        true
    }

    pub fn next_page(&mut self) -> bool {
        let next = self.page_index.saturating_add(1);

        if next >= self.pagination.len() {
            return false;
        }

        self.page_index = next;

        true
    }
}
