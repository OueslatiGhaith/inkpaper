use crate::reader::{TextSetting, TextSettings};

use super::ReaderState;

impl ReaderState {
    pub(crate) fn open_menu(&mut self) -> bool {
        if self.document.is_none() || self.chrome.menu_open {
            return false;
        }

        self.chrome.menu_open = true;

        true
    }

    pub(crate) fn close_menu(&mut self) -> bool {
        if !self.chrome.menu_open {
            return false;
        }

        self.chrome.menu_open = false;

        true
    }

    pub(crate) fn menu_open(&self) -> bool {
        self.chrome.menu_open
    }

    /// Flips a toggle row of the Text panel.
    pub(crate) fn toggle_text_setting(&mut self, setting: TextSetting) -> bool {
        if !self.chrome.menu_open {
            return false;
        }

        setting
            .toggle(self.text)
            .is_some_and(|text| self.request_text_settings(text))
    }

    /// Applies option `index` of `setting`'s picker in the drawer. The page under
    /// the drawer repaginates, so it previews the choice. Returns whether the
    /// settings changed.
    pub(crate) fn choose_text_option(&mut self, setting: TextSetting, index: usize) -> bool {
        if !self.chrome.menu_open {
            return false;
        }

        setting
            .choose(self.text, index)
            .is_some_and(|text| self.request_text_settings(text))
    }

    /// The settings the Text panel shows: the ones being repaginated to, then
    /// the current ones.
    pub(crate) fn menu_text_settings(&self) -> TextSettings {
        self.pending_repagination
            .map_or(self.text, |request| request.text)
    }
}
