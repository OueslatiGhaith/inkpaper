use alloc::vec::Vec;

use crate::ReadingProgress;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecentBooksRequest {
    Load,
}

#[derive(Debug, Default)]
pub(crate) struct RecentBooksState {
    entries: Vec<ReadingProgress>,
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

    pub(crate) fn apply_entries(&mut self, entries: Vec<ReadingProgress>) {
        self.entries = entries;
        self.error = false;
        self.revision = self.revision.wrapping_add(1);
    }

    pub(crate) fn apply_error(&mut self) {
        self.entries.clear();
        self.error = true;
        self.revision = self.revision.wrapping_add(1);
    }

    pub(crate) fn entries(&self) -> &[ReadingProgress] {
        &self.entries
    }

    pub(crate) fn entry(&self, index: usize) -> Option<&ReadingProgress> {
        self.entries.get(index)
    }

    pub(crate) const fn revision(&self) -> u64 {
        self.revision
    }

    pub(crate) const fn error(&self) -> bool {
        self.error
    }
}

#[cfg(test)]
mod tests {
    use alloc::string::String;

    use inkpaper_reader::ReadingPosition;

    use super::*;

    #[test]
    fn load_request_is_one_shot() {
        let mut state = RecentBooksState::default();

        state.request_load();

        assert_eq!(state.take_request(), Some(RecentBooksRequest::Load));

        assert_eq!(state.take_request(), None);
    }

    #[test]
    fn history_order_is_preserved() {
        let mut state = RecentBooksState::default();

        state.apply_entries(alloc::vec![
            ReadingProgress::new(String::from("/new.epub"), None, ReadingPosition::default()),
            ReadingProgress::new(String::from("/old.epub"), None, ReadingPosition::default()),
        ]);

        assert_eq!(state.entries()[0].path(), "/new.epub");

        assert_eq!(state.entries()[1].path(), "/old.epub");
    }
}
