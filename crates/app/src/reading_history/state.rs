use alloc::vec::Vec;

use super::ReadingHistoryEntry;
use crate::browser::PageTurn;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadingHistoryRequest {
    Load,
}

#[derive(Debug)]
pub(crate) struct ReadingHistoryState {
    entries: Vec<ReadingHistoryEntry>,
    pending: Option<ReadingHistoryRequest>,
    revision: u64,
    error: bool,
    /// the entry at the top of the page shown
    top: usize,
}

impl Default for ReadingHistoryState {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            // home is the initial screen, so request the persisted history immediately
            // at startup.
            pending: Some(ReadingHistoryRequest::Load),
            revision: 0,
            error: false,
            top: 0,
        }
    }
}

impl ReadingHistoryState {
    pub(crate) fn request_load(&mut self) {
        self.pending = Some(ReadingHistoryRequest::Load);

        self.error = false;
    }

    pub(crate) fn take_request(&mut self) -> Option<ReadingHistoryRequest> {
        self.pending.take()
    }

    pub(crate) fn apply_entries(&mut self, entries: Vec<ReadingHistoryEntry>) {
        self.entries = entries;
        self.error = false;
        self.top = 0;

        self.revision = self.revision.wrapping_add(1);
    }

    pub(crate) fn entries(&self) -> &[ReadingHistoryEntry] {
        &self.entries
    }

    pub(crate) fn current(&self) -> Option<&ReadingHistoryEntry> {
        self.entries.first()
    }

    pub(crate) fn entry(&self, index: usize) -> Option<&ReadingHistoryEntry> {
        self.entries.get(index)
    }

    /// The page shown when `rows_per_page` entries fit on one.
    pub(crate) fn page(&self, rows_per_page: usize) -> usize {
        (self.top / rows_per_page).min(self.page_count(rows_per_page) - 1)
    }

    pub(crate) fn page_count(&self, rows_per_page: usize) -> usize {
        self.entries.len().div_ceil(rows_per_page).max(1)
    }

    /// Returns false at either end of the history.
    pub(crate) fn turn_page(&mut self, turn: PageTurn, rows_per_page: usize) -> bool {
        let page = self.page(rows_per_page);
        let next = match turn {
            PageTurn::Previous => page.checked_sub(1),
            PageTurn::Next => Some(page + 1).filter(|&next| next < self.page_count(rows_per_page)),
        };

        let Some(next) = next else {
            return false;
        };

        self.top = next * rows_per_page;

        true
    }

    pub(crate) const fn revision(&self) -> u64 {
        self.revision
    }

    pub(crate) const fn error(&self) -> bool {
        self.error
    }
}
