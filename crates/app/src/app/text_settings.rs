use alloc::{string::String, vec::Vec};

use inkpaper_ui::prelude::*;

use super::{InkPaperApp, Screen};
use crate::{
    fonts::CardFont,
    reader::ReaderFont,
    screens::text_settings::{TextSettingsEvent, TextSettingsView},
};

impl InkPaperApp {
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

    /// What the Text Settings screen's taps mean. Like crosspoint, every change is
    /// saved as it is made.
    pub(super) fn on_text_settings_event(
        &mut self,
        _: Entity<TextSettingsView>,
        event: &TextSettingsEvent,
        cx: &mut Context<'_, Self>,
    ) {
        match *event {
            TextSettingsEvent::Back => self.navigate_back(cx),
            TextSettingsEvent::Change(text) => {
                if self.text_settings.set_draft(text) {
                    self.reader.save_text_settings(text);
                    cx.notify();
                }
            }
        }
    }

    /// The font the service should have loaded: the one Text Settings is
    /// previewing, or else the reader's.
    pub(crate) fn wanted_reader_font(&self) -> ReaderFont {
        if self.screen() == Screen::TextSettings {
            self.text_settings.draft().font()
        } else {
            self.reader.menu_text_settings().font()
        }
    }

    /// The card font read into memory, or none after it is freed.
    pub(crate) fn apply_card_font(&mut self, card: Option<CardFont>, cx: &mut Context<'_, Self>) {
        self.text_settings.set_card_font(card);

        // the reader's page paints in the font too
        cx.notify();
    }

    /// The name of the font the reader uses, for the drawer.
    pub(crate) fn reader_font_name(&self) -> String {
        self.text_settings
            .font_name(self.reader.menu_text_settings().font())
    }
}
