use alloc::{string::String, vec::Vec};

use inkpaper_ui::prelude::*;

use super::{InkPaperApp, Screen};
use crate::{reader::TextSettingsTab, reader_page::paint_page};

impl InkPaperApp {
    pub(crate) fn show_text_settings(&mut self, _: &ActivateEvent, cx: &mut Context<'_, Self>) {
        self.open_screen(Screen::TextSettings, cx);
    }

    /// The font families found on the card.
    pub(crate) fn apply_font_families(
        &mut self,
        families: Vec<String>,
        cx: &mut Context<'_, Self>,
    ) {
        if self.text_settings.set_families(families) {
            cx.notify();
        }
    }

    pub(crate) fn activate_text_settings_tab(
        &mut self,
        event: &ActivateEvent,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(&tab) = event
            .index()
            .and_then(|index| TextSettingsTab::ALL.get(index))
        else {
            return;
        };

        if self.text_settings.select_tab(tab) {
            cx.notify();
        }
    }

    pub(crate) fn activate_text_settings_row(
        &mut self,
        event: &ActivateEvent,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(index) = event.index() else {
            return;
        };

        if self.text_settings.activate_row(index) {
            self.save_text_settings();
            cx.notify();
        }
    }

    pub(crate) fn activate_text_settings_option(
        &mut self,
        event: &ActivateEvent,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(index) = event.index() else {
            return;
        };

        if self.text_settings.choose_option(index) {
            self.save_text_settings();
            cx.notify();
        }
    }

    pub(crate) fn activate_dismiss_text_settings_picker(
        &mut self,
        _: &ActivateEvent,
        cx: &mut Context<'_, Self>,
    ) {
        if self.text_settings.close_picker() {
            cx.notify();
        }
    }

    /// Like crosspoint, every change is saved as it is made.
    fn save_text_settings(&mut self) {
        self.reader.save_text_settings(self.text_settings.draft());
    }

    pub(crate) fn paint_text_settings_preview(&self, paint: &mut PaintCx<'_>) {
        if let Some(page) = self.text_settings.preview() {
            paint_page(page, None, paint);
        }
    }
}
