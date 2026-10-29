use alloc::vec::Vec;

use crate::{
    paging::{PageTurn, Pager},
    reader::{TableOfContents, TocEntry, toc::current_toc_index},
};

use super::{ReaderRequest, ReaderState};

impl ReaderState {
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

    /// Shows the chapter list's page holding the chapter being read.
    pub(crate) fn show_current_toc_page(&mut self) {
        self.toc_pager = Pager::showing(self.current_toc_index().unwrap_or(0));
    }

    fn toc_len(&self) -> usize {
        match &self.toc {
            TableOfContents::Loaded(entries) => entries.len(),
            _ => 0,
        }
    }

    /// The chapter list's page shown when `rows_per_page` entries fit on one.
    pub(crate) fn toc_page(&self, rows_per_page: usize) -> usize {
        self.toc_pager.page(self.toc_len(), rows_per_page)
    }

    pub(crate) fn toc_page_count(&self, rows_per_page: usize) -> usize {
        Pager::page_count(self.toc_len(), rows_per_page)
    }

    pub(crate) fn turn_toc_page(&mut self, turn: PageTurn, rows_per_page: usize) -> bool {
        self.toc_pager.turn(turn, self.toc_len(), rows_per_page)
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
        self.show_current_toc_page();

        true
    }

    pub(crate) fn apply_table_of_contents_error(&mut self, path: &str) -> bool {
        if path != self.path || self.toc != TableOfContents::Loading {
            return false;
        }

        self.toc = TableOfContents::Failed;

        true
    }
}
