use alloc::{string::String, vec::Vec};

use inkpaper_ui::prelude::*;

use super::InkPaperApp;
use crate::{
    dictionaries::{DictionaryRequest, LookupOutcome, LookupState},
    fonts::reader_family,
    paging::PageTurn,
    reader::layout_definition,
    reader_page::paint_page,
};

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

    /// Selects the word held at `position` on the reader's page, to look up
    /// once the touch ends. Returns whether there was a word and a dictionary
    /// to look it up in.
    pub(crate) fn select_word_at(&mut self, position: Point, cx: &mut Context<'_, Self>) -> bool {
        if self.reader.dictionary().is_none() {
            return false;
        }

        let card = self.text_settings.card_font();

        if self
            .reader
            .select_word(position.x.get(), position.y.get(), card)
            .is_none()
        {
            return false;
        }

        cx.notify();

        true
    }

    /// Grows or shrinks the selection from the word held to the one the
    /// touch dragged to at `position`.
    pub(crate) fn extend_selection_to(&mut self, position: Point, cx: &mut Context<'_, Self>) {
        let card = self.text_settings.card_font();

        if self
            .reader
            .extend_selection(position.x.get(), position.y.get(), card)
        {
            cx.notify();
        }
    }

    /// Looks the selected words up in the chosen dictionary.
    pub(crate) fn look_up_selection(&mut self, cx: &mut Context<'_, Self>) {
        let Some(dictionary) = self.reader.dictionary() else {
            return;
        };

        let Some(selection) = self.reader.selection() else {
            return;
        };

        self.lookup
            .start(dictionary, String::from(selection.word()));
        cx.notify();
    }

    pub(crate) fn take_dictionary_request(&mut self) -> Option<DictionaryRequest> {
        self.lookup.take_request()
    }

    /// The service is indexing the dictionary before it looks `word` up.
    pub(crate) fn apply_dictionary_indexing(&mut self, word: &str, cx: &mut Context<'_, Self>) {
        if self.lookup.finish(word, LookupState::Indexing) {
            cx.notify();
        }
    }

    /// The service's outcome for `word`, its definition laid out for the
    /// card in the reader's font.
    pub(crate) fn apply_dictionary_lookup(
        &mut self,
        word: &str,
        outcome: LookupOutcome,
        cx: &mut Context<'_, Self>,
    ) {
        if !self.lookup.waits_for(word) {
            return;
        }

        let state = match outcome {
            LookupOutcome::Found(definition) => LookupState::Found {
                headword: String::from(definition.headword()),
                pages: layout_definition(
                    &definition,
                    self.reader.text_settings(),
                    self.text_settings.card_font(),
                ),
                page: 0,
            },
            LookupOutcome::NotFound => LookupState::NotFound,
            LookupOutcome::Failed => LookupState::Failed,
        };

        if self.lookup.finish(word, state) {
            cx.notify();
        }
    }

    /// Closes the definition card and unselects its word. Returns whether it
    /// was open.
    pub(crate) fn close_word_lookup(&mut self, cx: &mut Context<'_, Self>) -> bool {
        let closed = self.lookup.close();

        if self.reader.clear_selection() || closed {
            cx.notify();
        }

        closed
    }

    pub(crate) fn activate_close_definition(
        &mut self,
        _: &ActivateEvent,
        cx: &mut Context<'_, Self>,
    ) {
        self.close_word_lookup(cx);
    }

    /// The card's left half turns back, its right half on.
    pub(crate) fn activate_definition_page(
        &mut self,
        event: &ActivateEvent,
        cx: &mut Context<'_, Self>,
    ) {
        let turn = match event.index() {
            Some(0) => PageTurn::Previous,
            Some(_) => PageTurn::Next,
            None => return,
        };

        self.turn_definition_page(turn, cx);
    }

    pub(crate) fn turn_definition_page(&mut self, turn: PageTurn, cx: &mut Context<'_, Self>) {
        if self.lookup.turn_page(turn) {
            cx.notify();
        }
    }

    pub(crate) fn paint_definition_page(&self, paint: &mut PaintCx<'_>) {
        let Some((page, ..)) = self.lookup.page() else {
            return;
        };

        let family = reader_family(
            self.reader.text_settings().font(),
            self.text_settings.card_font(),
        );

        let darkness = self.reader.text_settings().darkness().ink();

        paint_page(page, None, family, darkness, paint);
    }
}
