use alloc::string::String;

/// A key on the on-screen keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Key {
    /// Types a printable ASCII character; letters follow Shift.
    Char(u8),
    Shift,
    /// Shows the symbols.
    Symbols,
    /// Shows the letters again.
    Letters,
    /// Flips between the two symbol pages.
    Page,
    Delete,
    Submit,
}

// codes above the printable ASCII range, for keys that don't type
const SHIFT: usize = 0x80;
const SYMBOLS: usize = 0x81;
const LETTERS: usize = 0x82;
const PAGE: usize = 0x83;
const DELETE: usize = 0x84;
const SUBMIT: usize = 0x85;

impl Key {
    /// A number that identifies the key, used as its element id.
    pub(crate) const fn code(self) -> usize {
        match self {
            Self::Char(byte) => byte as usize,
            Self::Shift => SHIFT,
            Self::Symbols => SYMBOLS,
            Self::Letters => LETTERS,
            Self::Page => PAGE,
            Self::Delete => DELETE,
            Self::Submit => SUBMIT,
        }
    }

    pub(crate) const fn from_code(code: usize) -> Option<Self> {
        Some(match code {
            0x20..=0x7e => Self::Char(code as u8),
            SHIFT => Self::Shift,
            SYMBOLS => Self::Symbols,
            LETTERS => Self::Letters,
            PAGE => Self::Page,
            DELETE => Self::Delete,
            SUBMIT => Self::Submit,
            _ => return None,
        })
    }
}

const fn char_keys<const N: usize>(bytes: &[u8; N]) -> [Key; N] {
    let mut keys = [Key::Char(0); N];
    let mut index = 0;

    while index < N {
        keys[index] = Key::Char(bytes[index]);
        index += 1;
    }

    keys
}

const fn c(byte: u8) -> Key {
    Key::Char(byte)
}

const SPACE: Key = Key::Char(b' ');

// crosspoint's layout: a number row, three letter rows with Shift and Del at
// the ends of the third, then symbols, space and the submit key
const NUMBERS: &[Key] = &char_keys(b"1234567890");

const LETTER_ROWS: [&[Key]; 5] = [
    NUMBERS,
    &char_keys(b"qwertyuiop"),
    &char_keys(b"asdfghjkl"),
    &[
        Key::Shift,
        c(b'z'),
        c(b'x'),
        c(b'c'),
        c(b'v'),
        c(b'b'),
        c(b'n'),
        c(b'm'),
        Key::Delete,
    ],
    &[Key::Symbols, SPACE, Key::Submit],
];

// the two symbol pages hold all 32 printable ASCII symbols
const SYMBOL_ROWS: [[&[Key]; 5]; 2] = [
    [
        NUMBERS,
        &char_keys(b"!@#$%^&*()"),
        &char_keys(b"-_=+[]{}\\"),
        &[
            Key::Page,
            c(b';'),
            c(b':'),
            c(b'\''),
            c(b'"'),
            c(b','),
            c(b'.'),
            c(b'/'),
            Key::Delete,
        ],
        &[Key::Letters, SPACE, Key::Submit],
    ],
    [
        NUMBERS,
        &char_keys(b"?<>|~`"),
        &[],
        &[Key::Page, Key::Delete],
        &[Key::Letters, SPACE, Key::Submit],
    ],
];

/// Which keys the keyboard shows.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Layer {
    #[default]
    Letters,
    /// symbol page 0 or 1
    Symbols(u8),
}

/// What pressing a key did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KeyResult {
    Unchanged,
    Changed,
    /// The submit key was pressed with enough text.
    Submit,
}

/// The text typed on the on-screen keyboard. Text is only added or removed at
/// the end; there is no cursor to move.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct KeyboardState {
    text: String,
    layer: Layer,
    /// capitalizes the next letter only
    shift: bool,
    min_len: usize,
    max_len: usize,
}

impl KeyboardState {
    pub(crate) fn new(min_len: usize, max_len: usize) -> Self {
        Self {
            text: String::new(),
            layer: Layer::Letters,
            shift: false,
            min_len,
            max_len,
        }
    }

    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    pub(crate) const fn shift(&self) -> bool {
        self.shift
    }

    pub(crate) const fn layer(&self) -> Layer {
        self.layer
    }

    pub(crate) fn can_submit(&self) -> bool {
        self.text.len() >= self.min_len
    }

    /// The keys to show, top row first.
    pub(crate) fn rows(&self) -> [&'static [Key]; 5] {
        match self.layer {
            Layer::Letters => LETTER_ROWS,
            Layer::Symbols(page) => SYMBOL_ROWS[usize::from(page)],
        }
    }

    pub(crate) fn press(&mut self, key: Key) -> KeyResult {
        match key {
            Key::Char(byte) => {
                if self.text.len() >= self.max_len {
                    return KeyResult::Unchanged;
                }

                let byte = if self.shift {
                    byte.to_ascii_uppercase()
                } else {
                    byte
                };

                self.text.push(char::from(byte));
                self.shift = false;
            }

            Key::Shift => self.shift = !self.shift,

            Key::Symbols => {
                self.layer = Layer::Symbols(0);
                self.shift = false;
            }

            Key::Letters => self.layer = Layer::Letters,

            Key::Page => {
                let Layer::Symbols(page) = self.layer else {
                    return KeyResult::Unchanged;
                };

                self.layer = Layer::Symbols(1 - page);
            }

            Key::Delete => {
                if self.text.pop().is_none() {
                    return KeyResult::Unchanged;
                }
            }

            Key::Submit => {
                return if self.can_submit() {
                    KeyResult::Submit
                } else {
                    KeyResult::Unchanged
                };
            }
        }

        KeyResult::Changed
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use super::*;

    fn type_text(keyboard: &mut KeyboardState, text: &str) {
        for byte in text.bytes() {
            keyboard.press(Key::Char(byte));
        }
    }

    #[test]
    fn shift_capitalizes_only_the_next_letter() {
        let mut keyboard = KeyboardState::new(0, 64);

        keyboard.press(Key::Shift);
        type_text(&mut keyboard, "ab");
        keyboard.press(Key::Shift);
        keyboard.press(Key::Char(b'1'));
        keyboard.press(Key::Char(b'c'));

        assert_eq!(keyboard.text(), "Ab1c");
    }

    #[test]
    fn text_stays_within_the_limits() {
        let mut keyboard = KeyboardState::new(8, 10);

        type_text(&mut keyboard, "passwor");
        assert_eq!(keyboard.press(Key::Submit), KeyResult::Unchanged);

        type_text(&mut keyboard, "d123");
        assert_eq!(keyboard.text(), "password12");
        assert_eq!(keyboard.press(Key::Submit), KeyResult::Submit);

        keyboard.press(Key::Delete);
        keyboard.press(Key::Delete);
        keyboard.press(Key::Delete);
        assert_eq!(keyboard.text(), "passwor");
        assert!(!keyboard.can_submit());
    }

    #[test]
    fn deleting_nothing_changes_nothing() {
        let mut keyboard = KeyboardState::new(0, 64);

        assert_eq!(keyboard.press(Key::Delete), KeyResult::Unchanged);
    }

    #[test]
    fn symbols_flip_between_pages_and_back_to_letters() {
        let mut keyboard = KeyboardState::new(0, 64);

        keyboard.press(Key::Symbols);
        assert_eq!(keyboard.layer(), Layer::Symbols(0));

        keyboard.press(Key::Page);
        assert_eq!(keyboard.layer(), Layer::Symbols(1));

        keyboard.press(Key::Page);
        assert_eq!(keyboard.layer(), Layer::Symbols(0));

        keyboard.press(Key::Letters);
        assert_eq!(keyboard.layer(), Layer::Letters);
    }

    #[test]
    fn the_symbol_pages_cover_every_printable_symbol_once() {
        let mut keyboard = KeyboardState::new(0, 64);
        keyboard.press(Key::Symbols);

        let mut symbols = Vec::new();
        for _ in 0..2 {
            for row in &keyboard.rows()[1..4] {
                symbols.extend(row.iter().filter_map(|key| match key {
                    Key::Char(byte) => Some(*byte),
                    _ => None,
                }));
            }

            keyboard.press(Key::Page);
        }

        symbols.sort_unstable();

        let expected: Vec<u8> = (0x21..=0x7e)
            .filter(|byte: &u8| byte.is_ascii_punctuation())
            .collect();
        assert_eq!(symbols, expected);
    }

    #[test]
    fn every_key_is_found_again_from_its_code() {
        let mut keyboard = KeyboardState::new(0, 64);

        for layer in [Key::Letters, Key::Symbols, Key::Page] {
            keyboard.press(layer);

            for key in keyboard.rows().iter().flat_map(|row| row.iter()) {
                assert_eq!(Key::from_code(key.code()), Some(*key));
            }
        }
    }
}
