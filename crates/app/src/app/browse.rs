use inkpaper_ui::prelude::*;

use super::{
    InkPaperApp, Screen,
    navigation::{Back, Entry, ScreenLifecycle},
};
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

pub(super) struct BrowseFilesRoute;

impl ScreenLifecycle for BrowseFilesRoute {
    fn enter(&self, app: &mut InkPaperApp, entry: Entry) {
        // returning from the reader keeps the listing already shown
        if entry == Entry::Opened {
            app.browser.request_current_directory();
        }
    }

    fn back(&self, app: &mut InkPaperApp, cx: &mut Context<'_, InkPaperApp>) -> Back {
        if app.browser.request_parent() {
            cx.notify();
            return Back::Handled;
        }

        Back::Leave
    }
}
