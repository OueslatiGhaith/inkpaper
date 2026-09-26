use inkpaper_ui::prelude::*;

use super::{InkPaperApp, Screen};
use crate::{BrowseListing, BrowseRequest};

impl InkPaperApp {
    pub(crate) fn take_browse_request(&mut self) -> Option<BrowseRequest> {
        self.browser.take_request()
    }

    pub(crate) fn apply_browse_listing(
        &mut self,
        listing: BrowseListing,
        cx: &mut Context<'_, Self>,
    ) {
        self.browser.apply_listing(listing);
        cx.notify();
    }

    pub(crate) fn apply_browse_error(&mut self, cx: &mut Context<'_, Self>) {
        self.browser.apply_error();
        cx.notify();
    }

    pub(super) fn activate_browse_entry(&mut self, index: usize, cx: &mut Context<'_, Self>) {
        if self.browser.request_entry(index) {
            cx.notify();
            return;
        }

        let Some(file) = self.browser.file_at(index) else {
            return;
        };

        // EPUB is the first reader format. Other file types remain visible in Browse Files
        // but intentionally do nothing until their corresponding reader/viewer exists.
        if !file.is_epub() {
            return;
        }

        let (path, title) = file.into_reader_parts();

        self.reader.open(path, title);

        self.open_screen(Screen::Reader, cx);
    }
}
