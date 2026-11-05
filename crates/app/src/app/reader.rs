use alloc::{string::String, vec::Vec};
use inkpaper_epub::SpineIndex;
use inkpaper_ui::prelude::*;

use super::{InkPaperApp, Screen};
use crate::{
    ReaderChapter, ReaderChapterDirection, ReaderDocument, ReaderPreferences,
    components::{
        reader_menu::{DrawerEvent, ReaderDrawer},
        toast::{Toast, ToastId},
    },
    fonts::reader_family,
    reader::{JumpTarget, TableOfContents, TextSettings, TocEntry},
    reader_page::paint_reader_page,
};

impl InkPaperApp {
    /// Opens the book at `path`. Like crosspoint, the screen it was opened
    /// from stays under a toast until the book has opened, then the reader
    /// shows it. A book that can't be opened leaves the screen as it was.
    pub(crate) fn open_book(&mut self, path: String, title: String, cx: &mut Context<'_, Self>) {
        self.reader.open(path, title);
        self.show_toast(Toast::new(ToastId::OpeningBook, "Loading"), cx);
    }

    /// Reopens at boot the book read when the device went to sleep. Nothing
    /// shows before it, so the screen goes from the sleep screen straight to
    /// its page. A book that can't be opened leaves Home under a toast.
    pub(crate) fn resume_book(&mut self, path: String) {
        let title = self
            .reading_history
            .entries()
            .iter()
            .find(|entry| entry.path() == path)
            .map(|entry| String::from(entry.display_title()))
            .unwrap_or_default();

        self.reader.resume(path, title);
    }

    /// The book to reopen at boot: the one being read, in the reader or a
    /// screen opened from it like its table of contents.
    pub(crate) fn book_to_resume(&self) -> Option<String> {
        let reading = self.is_open(Screen::Reader) && self.reader.document().is_some();

        reading.then(|| String::from(self.reader.path()))
    }

    pub(crate) fn apply_reader_document(
        &mut self,
        document: ReaderDocument,
        cx: &mut Context<'_, Self>,
    ) -> bool {
        let changed = self.reader.apply_document(document);

        if changed {
            self.dismiss_toast(ToastId::OpeningBook, cx);
            self.open_screen(Screen::Reader, cx);
            cx.notify();
        }

        changed
    }

    pub(crate) fn apply_reader_chapter(
        &mut self,
        path: String,
        from: SpineIndex,
        direction: ReaderChapterDirection,
        chapter: ReaderChapter,
        cx: &mut Context<'_, Self>,
    ) -> bool {
        let changed = self.reader.apply_chapter(&path, from, direction, chapter);

        if changed {
            cx.notify();
        }

        changed
    }

    pub(crate) fn finish_reader_chapter_request(
        &mut self,
        path: String,
        from: SpineIndex,
        direction: ReaderChapterDirection,
    ) -> bool {
        self.reader.finish_chapter_request(&path, from, direction)
    }

    pub(crate) fn apply_reader_error(&mut self, path: String, cx: &mut Context<'_, Self>) -> bool {
        let changed = self.reader.apply_error(&path);

        if changed {
            let failed = Toast::new(ToastId::BookFailed, "Could not open EPUB").autohide();

            self.show_toast(failed, cx);
        }

        changed
    }

    pub(crate) fn activate_previous_reader_page(
        &mut self,
        _: &ActivateEvent,
        cx: &mut Context<'_, Self>,
    ) {
        self.reader_previous_page(cx);
    }

    pub(crate) fn activate_next_reader_page(
        &mut self,
        _: &ActivateEvent,
        cx: &mut Context<'_, Self>,
    ) {
        self.reader_next_page(cx);
    }

    pub(crate) fn reader_previous_page(&mut self, cx: &mut Context<'_, Self>) {
        if self.reader.previous_page() {
            cx.notify();
        }
    }

    pub(crate) fn reader_next_page(&mut self, cx: &mut Context<'_, Self>) {
        if self.reader.next_page() {
            cx.notify();
        }
    }

    /// What the reader drawer's taps mean.
    pub(super) fn on_reader_drawer_event(
        &mut self,
        _: Entity<ReaderDrawer>,
        event: &DrawerEvent,
        cx: &mut Context<'_, Self>,
    ) {
        let changed = match *event {
            DrawerEvent::Close => return self.close_reader_menu(cx),
            DrawerEvent::OpenTextSettings => {
                self.reader.close_menu();
                self.open_screen(Screen::TextSettings, cx);
                return;
            }
            DrawerEvent::SelectChapter => return self.show_table_of_contents(cx),
            DrawerEvent::ToggleTextSetting(setting) => self.reader.toggle_text_setting(setting),
            // the page under the drawer repaginates, so it previews the choice
            DrawerEvent::ChooseTextOption(setting, index) => {
                self.reader.choose_text_option(setting, index)
            }
            DrawerEvent::FollowLink(index) => self.reader.follow_page_link(index),
        };

        if changed {
            cx.notify();
        }
    }

    /// Opens the drawer on its Text tab.
    pub(crate) fn open_reader_menu(&mut self, cx: &mut Context<'_, Self>) {
        if self.reader.open_menu() {
            self.reader_menu.update(cx, ReaderDrawer::reset);
            cx.notify();
        }
    }

    pub(crate) fn close_reader_menu(&mut self, cx: &mut Context<'_, Self>) {
        if self.reader.close_menu() {
            cx.notify();
        }
    }

    /// Opens the chapter list; like crosspoint, the drawer does not come back.
    fn show_table_of_contents(&mut self, cx: &mut Context<'_, Self>) {
        self.reader.close_menu();
        self.open_screen(Screen::TableOfContents, cx);
    }

    /// Jumps to a chapter list entry and returns to the reader. Entries that
    /// lead nowhere just close the list.
    pub(crate) fn activate_toc_entry(&mut self, event: &ActivateEvent, cx: &mut Context<'_, Self>) {
        let Some(index) = event.index() else {
            return;
        };

        let target = match self.reader.table_of_contents() {
            TableOfContents::Loaded(entries) => {
                entries.get(index).and_then(TocEntry::target).cloned()
            }

            _ => None,
        };

        if let Some(target) = target {
            self.reader.jump_to(target.spine, target.anchor);
        }

        self.navigate_back(cx);
    }

    pub(crate) fn apply_reader_table_of_contents(
        &mut self,
        path: String,
        entries: Vec<TocEntry>,
        cx: &mut Context<'_, Self>,
    ) {
        if self.reader.apply_table_of_contents(&path, entries) {
            cx.notify();
        }
    }

    pub(crate) fn apply_reader_table_of_contents_error(
        &mut self,
        path: String,
        cx: &mut Context<'_, Self>,
    ) {
        if self.reader.apply_table_of_contents_error(&path) {
            cx.notify();
        }
    }

    pub(crate) fn activate_open_reader_menu(
        &mut self,
        _: &ActivateEvent,
        cx: &mut Context<'_, Self>,
    ) {
        self.open_reader_menu(cx);
    }

    pub(crate) fn activate_close_reader_menu(
        &mut self,
        _: &ActivateEvent,
        cx: &mut Context<'_, Self>,
    ) {
        self.close_reader_menu(cx);
    }

    pub(crate) fn apply_reader_repagination(
        &mut self,
        path: String,
        spine: SpineIndex,
        text: TextSettings,
        chapter: ReaderChapter,
        cx: &mut Context<'_, Self>,
    ) -> bool {
        let changed = self
            .reader
            .apply_repaginated_chapter(&path, spine, text, chapter);

        if changed {
            cx.notify();
        }

        changed
    }

    pub(crate) fn finish_reader_repagination_request(
        &mut self,
        path: String,
        spine: SpineIndex,
        text: TextSettings,
    ) -> bool {
        self.reader.finish_repagination_request(&path, spine, text)
    }

    pub(crate) fn apply_reader_jump(
        &mut self,
        path: String,
        target: JumpTarget,
        chapter: ReaderChapter,
        page_index: usize,
        cx: &mut Context<'_, Self>,
    ) -> bool {
        let changed = self.reader.apply_jump(&path, target, chapter, page_index);

        if changed {
            cx.notify();
        }

        changed
    }

    pub(crate) fn finish_reader_jump_request(&mut self, path: String, target: JumpTarget) -> bool {
        self.reader.finish_jump_request(&path, target)
    }

    /// Follows a link tapped on the page; the element's index says which.
    pub(crate) fn activate_reader_link(
        &mut self,
        event: &ActivateEvent,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(index) = event.index() else {
            return;
        };

        if self.reader.follow_link(index) {
            cx.notify();
        }
    }

    pub(crate) fn apply_reader_preferences(
        &mut self,
        preferences: ReaderPreferences,
        cx: &mut Context<'_, Self>,
    ) -> bool {
        let changed = self.reader.apply_preferences(preferences);

        if changed {
            cx.notify();
        }

        changed
    }

    pub(crate) fn paint_reader_page(&self, paint: &mut PaintCx<'_>) {
        let Some(document) = self.reader.document() else {
            return;
        };

        let Some(page) = self.reader.page() else {
            return;
        };

        let family = reader_family(
            self.reader.text_settings().font(),
            self.text_settings.card_font(),
        );

        let selected = self
            .reader
            .selection()
            .map_or(&[][..], |selection| selection.pieces());

        let text = self.reader.text_settings();

        paint_reader_page(page, document, family, text, selected, paint);
    }
}
