use alloc::{string::String, vec::Vec};
use inkpaper_epub::SpineIndex;
use inkpaper_ui::prelude::*;

use super::{InkPaperApp, Screen};
use crate::{
    ReaderChapter, ReaderChapterDirection, ReaderDocument, ReaderPreferences,
    ReaderPreferencesRequest, ReaderRequest,
    reader::{JumpTarget, ReaderMenuTab, TableOfContents, TextSetting, TextSettings, TocEntry},
    reader_page::paint_reader_page,
};

impl InkPaperApp {
    pub(crate) fn take_reader_request(&mut self) -> Option<ReaderRequest> {
        self.reader.take_request()
    }

    pub(crate) fn apply_reader_document(
        &mut self,
        document: ReaderDocument,
        cx: &mut Context<'_, Self>,
    ) -> bool {
        let changed = self.reader.apply_document(document);

        if changed {
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
            cx.notify();
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

    /// renders the reader's drawer again, for changes nothing else shows
    pub(crate) fn refresh_reader_menu(&self, cx: &mut Context<'_, Self>) {
        self.reader_menu.update(cx, |_, cx| cx.notify()).ok();
    }

    pub(crate) fn open_reader_menu(&mut self, cx: &mut Context<'_, Self>) {
        if self.reader.open_menu() {
            cx.notify();
        }
    }

    pub(crate) fn close_reader_menu(&mut self, cx: &mut Context<'_, Self>) {
        if self.reader.close_menu() {
            cx.notify();
        }
    }

    pub(crate) fn activate_reader_text_tab(
        &mut self,
        _: &ActivateEvent,
        cx: &mut Context<'_, Self>,
    ) {
        if self.reader.select_menu_tab(ReaderMenuTab::Text) {
            self.refresh_reader_menu(cx);
        }
    }

    pub(crate) fn activate_reader_more_tab(
        &mut self,
        _: &ActivateEvent,
        cx: &mut Context<'_, Self>,
    ) {
        if self.reader.select_menu_tab(ReaderMenuTab::More) {
            self.refresh_reader_menu(cx);
        }
    }

    /// Opens the chapter list; like crosspoint, the drawer does not come back.
    pub(crate) fn show_table_of_contents(&mut self, _: &ActivateEvent, cx: &mut Context<'_, Self>) {
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

    /// Opens the picker of a Text panel row, or flips a toggle row. The Font
    /// row opens the Text Settings screen, like crosspoint's.
    pub(crate) fn activate_reader_text_row(
        &mut self,
        event: &ActivateEvent,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(&setting) = event
            .index()
            .and_then(|index| TextSetting::PANEL.get(index))
        else {
            return;
        };

        if setting == TextSetting::Font {
            self.reader.close_menu();
            self.open_screen(Screen::TextSettings, cx);
            return;
        }

        // like crosspoint's checkbox rows, toggles flip in place
        let changed = if setting.is_toggle() {
            self.reader.toggle_text_setting(setting)
        } else {
            self.reader.open_text_picker(setting)
        };

        if changed {
            cx.notify();
            self.refresh_reader_menu(cx);
        }
    }

    pub(crate) fn activate_reader_text_option(
        &mut self,
        event: &ActivateEvent,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(index) = event.index() else {
            return;
        };

        if self.reader.choose_text_option(index) {
            cx.notify();
            self.refresh_reader_menu(cx);
        }
    }

    pub(crate) fn activate_dismiss_reader_text_picker(
        &mut self,
        _: &ActivateEvent,
        cx: &mut Context<'_, Self>,
    ) {
        if self.reader.close_text_picker() {
            cx.notify();
        }
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

    pub(crate) fn take_reader_preferences_request(&mut self) -> Option<ReaderPreferencesRequest> {
        self.reader.take_preferences_request()
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

        paint_reader_page(page, document, paint);
    }
}
