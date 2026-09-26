use alloc::string::String;
use inkpaper_epub::SpineIndex;
use inkpaper_ui::prelude::*;

use super::InkPaperApp;
use crate::{
    ReaderChapter, ReaderChapterDirection, ReaderDocument, ReaderPreferences,
    ReaderPreferencesRequest, ReaderRequest, reader_page::paint_reader_page,
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

    pub(crate) fn activate_decrease_reader_font_size(
        &mut self,
        _: &ActivateEvent,
        _: &mut Context<'_, Self>,
    ) {
        self.reader.decrease_font_size();
    }

    pub(crate) fn activate_increase_reader_font_size(
        &mut self,
        _: &ActivateEvent,
        _: &mut Context<'_, Self>,
    ) {
        self.reader.increase_font_size();
    }

    pub(crate) fn apply_reader_repagination(
        &mut self,
        path: String,
        spine: SpineIndex,
        font_size: u16,
        chapter: ReaderChapter,
        cx: &mut Context<'_, Self>,
    ) -> bool {
        let changed = self
            .reader
            .apply_repaginated_chapter(&path, spine, font_size, chapter);

        if changed {
            cx.notify();
        }

        changed
    }

    pub(crate) fn finish_reader_repagination_request(
        &mut self,
        path: String,
        spine: SpineIndex,
        font_size: u16,
    ) -> bool {
        self.reader
            .finish_repagination_request(&path, spine, font_size)
    }

    pub(crate) fn apply_reader_jump(
        &mut self,
        path: String,
        spine: SpineIndex,
        anchor: Option<String>,
        chapter: ReaderChapter,
        page_index: usize,
        cx: &mut Context<'_, Self>,
    ) -> bool {
        let changed = self
            .reader
            .apply_jump(&path, spine, anchor, chapter, page_index);

        if changed {
            cx.notify();
        }

        changed
    }

    pub(crate) fn finish_reader_jump_request(
        &mut self,
        path: String,
        spine: SpineIndex,
        anchor: Option<String>,
    ) -> bool {
        self.reader.finish_jump_request(&path, spine, anchor)
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
