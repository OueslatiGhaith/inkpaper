use alloc::vec::Vec;

use super::ReadingHistoryEntry;

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

        self.revision = self.revision.wrapping_add(1);
    }

    pub(crate) fn apply_error(&mut self) {
        // keep the previous snapshot if one exists.
        // A transient storage failure should not erase already-known Home metadata.
        self.error = true;
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

    pub(crate) const fn revision(&self) -> u64 {
        self.revision
    }

    pub(crate) const fn error(&self) -> bool {
        self.error
    }
}
