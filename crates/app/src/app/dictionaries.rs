use alloc::{string::String, vec::Vec};

use inkpaper_ui::prelude::*;

use super::InkPaperApp;

impl InkPaperApp {
    /// The dictionaries found on the card.
    pub(crate) fn apply_dictionaries(&mut self, names: Vec<String>, cx: &mut Context<'_, Self>) {
        if self.dictionaries.set_names(names) {
            cx.notify();
        }
    }

    pub(crate) fn activate_dictionary_setting(
        &mut self,
        _: &ActivateEvent,
        cx: &mut Context<'_, Self>,
    ) {
        if self.dictionaries.open_picker() {
            cx.notify();
        }
    }

    /// The picker's first option turns lookups off; the others choose a
    /// dictionary.
    pub(crate) fn activate_dictionary_option(
        &mut self,
        event: &ActivateEvent,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(index) = event.index() else {
            return;
        };

        let dictionary = match index.checked_sub(1) {
            None => None,
            Some(index) if index < self.dictionaries.names().len() => u8::try_from(index).ok(),
            Some(_) => return,
        };

        self.reader.choose_dictionary(dictionary);
        self.dictionaries.close_picker();

        cx.notify();
    }

    pub(crate) fn activate_dismiss_dictionary_picker(
        &mut self,
        _: &ActivateEvent,
        cx: &mut Context<'_, Self>,
    ) {
        if self.dictionaries.close_picker() {
            cx.notify();
        }
    }

    /// The name of the dictionary words are looked up in.
    pub(crate) fn dictionary_name(&self) -> Option<&str> {
        let index = usize::from(self.reader.dictionary()?);

        self.dictionaries.names().get(index).map(String::as_str)
    }
}
