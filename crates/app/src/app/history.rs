use alloc::{string::String, vec::Vec};
use inkpaper_ui::{ImageSource, prelude::*};

use super::{InkPaperApp, Screen};
use crate::{ReadingHistoryEntry, ReadingHistoryRequest};

impl InkPaperApp {
    pub(crate) fn activate_current_book(&mut self, _: &ActivateEvent, cx: &mut Context<'_, Self>) {
        self.open_history_entry(0, cx);
    }

    pub(crate) fn activate_recent_book(
        &mut self,
        event: &ActivateEvent,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(index) = event.index() else {
            return;
        };

        self.open_history_entry(index, cx);
    }

    fn open_history_entry(&mut self, index: usize, cx: &mut Context<'_, Self>) {
        let Some(entry) = self.reading_history.entry(index) else {
            return;
        };

        let path = String::from(entry.path());
        let title = String::from(entry.display_title());

        self.reader.open(path, title);
        self.open_screen(Screen::Reader, cx);
    }

    pub(crate) fn take_reading_history_request(&mut self) -> Option<ReadingHistoryRequest> {
        self.reading_history.take_request()
    }

    pub(crate) fn apply_reading_history(
        &mut self,
        entries: Vec<ReadingHistoryEntry>,
        cx: &mut Context<'_, Self>,
    ) {
        self.reading_history.apply_entries(entries);

        if self.screen() == Screen::Home {
            self.request_current_cover();
        }

        cx.notify();
    }

    /// Asks for the current book's cover, for the home screen.
    pub(crate) fn request_current_cover(&mut self) {
        if let Some(entry) = self.reading_history.current() {
            self.cover.request(entry.path());
        }
    }

    pub(crate) fn take_cover_request(&mut self) -> Option<String> {
        self.cover.take_request()
    }

    pub(crate) fn apply_cover(
        &mut self,
        path: String,
        source: Option<ImageSource>,
        cx: &mut Context<'_, Self>,
    ) {
        if self.cover.apply(path, source) {
            cx.notify();
        }
    }
}
