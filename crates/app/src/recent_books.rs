use alloc::vec::Vec;

use crate::ReadingHistoryEntry;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecentBooksRequest {
    Load,
}

#[derive(Debug, Default)]
pub(crate) struct RecentBooksState {
    entries: Vec<ReadingHistoryEntry>,
    pending: Option<RecentBooksRequest>,
    revision: u64,
    error: bool,
}

impl RecentBooksState {
    pub(crate) fn request_load(&mut self) {
        self.pending = Some(RecentBooksRequest::Load);
        self.error = false;
    }

    pub(crate) fn take_request(&mut self) -> Option<RecentBooksRequest> {
        self.pending.take()
    }

    pub(crate) fn apply_entries(&mut self, entries: Vec<ReadingHistoryEntry>) {
        self.entries = entries;
        self.error = false;
        self.revision = self.revision.wrapping_add(1);
    }

    pub(crate) fn apply_error(&mut self) {
        self.entries.clear();
        self.error = true;
        self.revision = self.revision.wrapping_add(1);
    }

    pub(crate) fn entries(&self) -> &[ReadingHistoryEntry] {
        &self.entries
    }

    pub(crate) fn entry(&self, index: usize) -> Option<&ReadingHistoryEntry> {
        self.entries.get(index)
    }

    pub(crate) const fn revision(&self) -> u64 {
        self.revision
    }

    pub(crate) const fn error(&self) -> bool {
        self.error
    }
}
