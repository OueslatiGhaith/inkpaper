use inkpaper_reader::Page;

use crate::{
    fonts::{CardFont, card_for},
    reader::{
        WordSelection,
        measurer::{ReaderFonts, ReaderMeasurer},
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
        let selection = self.measure_at(x, y, card, |page, x, y, measurer| {
            word_at(page, x, y, measurer)
        })?;

        Some(self.selection.insert(selection).word())
    }

    /// Grows or shrinks the selection from the word held to the one at `x`
    /// and `y` on the screen. Returns whether it changed.
    pub(crate) fn extend_selection(&mut self, x: i32, y: i32, card: Option<&CardFont>) -> bool {
        let Some(selection) = self.selection() else {
            return false;
        };

        let extended = self.measure_at(x, y, card, |page, x, y, measurer| {
            extend_to(selection, page, x, y, measurer)
        });

        extended
            .map(|extended| self.selection = Some(extended))
            .is_some()
    }

    /// What `measure` finds at `x` and `y` on the screen, given them on the
    /// page shown, and a measurer for the font it was laid out in.
    fn measure_at<T>(
        &self,
        x: i32,
        y: i32,
        card: Option<&CardFont>,
        measure: impl FnOnce(&Page<'_>, u32, u32, &mut ReaderMeasurer<'_>) -> Option<T>,
    ) -> Option<T> {
        let bounds = self.text.margin().page_bounds();
        let x = u32::try_from(x).ok()?.checked_sub(bounds.left)?;
        let y = u32::try_from(y).ok()?.checked_sub(bounds.top)?;

        let fonts = ReaderFonts::new(card_for(self.text.font(), card)).ok()?;
        let mut measurer = ReaderMeasurer::new(&fonts, Default::default());

        measure(self.page()?, x, y, &mut measurer)
    }

    /// Unselects the words. Returns whether any were selected.
    pub(crate) fn clear_selection(&mut self) -> bool {
        self.selection.take().is_some()
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
