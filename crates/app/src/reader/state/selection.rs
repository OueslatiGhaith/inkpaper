use crate::{
    fonts::{CardFont, card_for},
    reader::{
        WordSelection,
        selection::{extend_to, word_at},
    },
};

use super::ReaderState;

impl ReaderState {
    pub(crate) const fn dictionary(&self) -> Option<u8> {
        self.dictionary
    }

    /// Selects the word at `x` and `y` on the screen, measured in the font
    /// the page was laid out in, `card` when it's the reader's. Returns it.
    pub(crate) fn select_word(&mut self, x: i32, y: i32, card: Option<&CardFont>) -> Option<&str> {
        let (x, y) = self.page_point(x, y)?;
        let page = self
            .document
            .as_ref()
            .and_then(|document| document.page(self.page_index))?;
        let fonts = self
            .selection_fonts
            .get_or_init(card_for(self.text.font(), card))
            .ok()?;

        let selection = word_at(page, x, y, fonts)?;

        Some(self.selection.insert(selection).word())
    }

    /// Grows or shrinks the selection from the word held to the one at `x`
    /// and `y` on the screen. Returns whether it changed.
    pub(crate) fn extend_selection(&mut self, x: i32, y: i32, card: Option<&CardFont>) -> bool {
        let Some((x, y)) = self.page_point(x, y) else {
            return false;
        };
        let Some(page) = self
            .document
            .as_ref()
            .and_then(|document| document.page(self.page_index))
        else {
            return false;
        };
        let Some(selection) = self
            .selection
            .as_ref()
            .filter(|selection| selection.position() == page.position())
        else {
            return false;
        };
        let Ok(fonts) = self
            .selection_fonts
            .get_or_init(card_for(self.text.font(), card))
        else {
            return false;
        };

        let extended = extend_to(selection, page, x, y, fonts);

        extended
            .map(|extended| self.selection = Some(extended))
            .is_some()
    }

    fn page_point(&self, x: i32, y: i32) -> Option<(u32, u32)> {
        let bounds = self.text.margin().page_bounds();
        let x = u32::try_from(x).ok()?.checked_sub(bounds.left)?;
        let y = u32::try_from(y).ok()?.checked_sub(bounds.top)?;

        Some((x, y))
    }

    /// Unselects the words. Returns whether any were selected.
    pub(crate) fn clear_selection(&mut self) -> bool {
        self.selection.take().is_some()
    }

    /// Releases cached selection fonts when leaving the reader.
    pub(crate) fn clear_selection_fonts(&mut self) {
        self.selection_fonts.clear();
    }

    /// The selected words, while their page is the one shown.
    pub(crate) fn selection(&self) -> Option<&WordSelection> {
        let position = self.page()?.position();

        self.selection
            .as_ref()
            .filter(|selection| selection.position() == position)
    }

    /// Looks words up in `dictionary` from now on, and saves it. Returns
    /// whether it changed.
    pub(crate) fn choose_dictionary(&mut self, dictionary: Option<u8>) -> bool {
        if dictionary == self.dictionary {
            return false;
        }

        self.dictionary = dictionary;
        self.queue_preferences_update();

        true
    }
}
