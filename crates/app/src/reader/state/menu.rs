use crate::reader::{TextSetting, TextSettings};

use super::{ReaderMenuTab, ReaderState};

impl ReaderState {
    pub(crate) fn open_menu(&mut self) -> bool {
        if self.document.is_none() || self.chrome.menu_open {
            return false;
        }

        self.chrome.menu_open = true;
        self.chrome.menu_tab = ReaderMenuTab::Text;
        self.chrome.text_picker = None;
        self.chrome.link_picker = false;

        true
    }

    pub(crate) fn close_menu(&mut self) -> bool {
        if !self.chrome.menu_open {
            return false;
        }

        self.chrome.menu_open = false;
        self.chrome.text_picker = None;
        self.chrome.link_picker = false;

        true
    }

    pub(crate) fn menu_open(&self) -> bool {
        self.chrome.menu_open
    }

    pub(crate) const fn menu_tab(&self) -> ReaderMenuTab {
        self.chrome.menu_tab
    }

    pub(crate) fn select_menu_tab(&mut self, tab: ReaderMenuTab) -> bool {
        if !self.chrome.menu_open || self.chrome.menu_tab == tab {
            return false;
        }

        self.chrome.menu_tab = tab;
        self.chrome.text_picker = None;
        self.chrome.link_picker = false;

        true
    }

    pub(crate) const fn text_picker(&self) -> Option<TextSetting> {
        self.chrome.text_picker
    }

    pub(crate) fn open_text_picker(&mut self, setting: TextSetting) -> bool {
        if !self.chrome.menu_open || self.chrome.text_picker.is_some() {
            return false;
        }

        self.chrome.text_picker = Some(setting);

        true
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

    pub(crate) fn close_text_picker(&mut self) -> bool {
        self.chrome.text_picker.take().is_some()
    }

    /// Applies option `index` of the open picker and closes it. The page under
    /// the drawer repaginates, so it previews the choice.
    pub(crate) fn choose_text_option(&mut self, index: usize) -> bool {
        let Some(setting) = self.chrome.text_picker.take() else {
            return false;
        };

        if let Some(text) = setting.choose(self.text, index) {
            self.request_text_settings(text);
        }

        true
    }

    /// The settings the Text panel shows: the ones being repaginated to, then
    /// the current ones.
    pub(crate) fn menu_text_settings(&self) -> TextSettings {
        self.pending_repagination
            .map_or(self.text, |request| request.text)
    }
}
