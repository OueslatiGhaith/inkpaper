use alloc::{string::String, vec::Vec};

use minicbor::{Decode, Encode};

use super::{LineSpacing, ParagraphAlignment, ReaderFont, ScreenMargin, TextSettings};
use crate::storage::{self, StorageError};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReaderPreferences {
    text: TextSettings,
}

impl ReaderPreferences {
    pub const fn new(text: TextSettings) -> Self {
        Self { text }
    }

    pub const fn text(self) -> TextSettings {
        self.text
    }

    /// Saves the font by `font_name`, its family's name, since the card's
    /// families may change before the next start.
    pub fn encode(self, font_name: Option<&str>) -> Result<Vec<u8>, ReaderPreferencesError> {
        let stored = StoredReaderPreferences {
            font_size: self.text.font_size(),
            line_spacing: self.text.line_spacing().index(),
            margin: self.text.margin().px(),
            alignment: self.text.alignment().index(),
            paragraph_indent: self.text.paragraph_indent(),
            paragraph_spacing: self.text.paragraph_spacing(),
            embedded_style: self.text.embedded_style(),
            hyphenation: Some(self.text.hyphenation()),
            font: font_name.map(String::from),
        };

        Ok(storage::encode(&stored)?)
    }

    /// `card_font` finds a saved family on the card; like crosspoint, one
    /// that's gone falls back to the built-in font.
    pub fn decode(
        bytes: &[u8],
        card_font: impl Fn(&str) -> Option<ReaderFont>,
    ) -> Result<Self, ReaderPreferencesError> {
        let stored: StoredReaderPreferences = storage::decode(bytes)?;

        let line_spacing = LineSpacing::from_index(stored.line_spacing).ok_or(
            ReaderPreferencesError::InvalidLineSpacing(stored.line_spacing),
        )?;

        let margin = ScreenMargin::new(stored.margin)
            .ok_or(ReaderPreferencesError::InvalidMargin(stored.margin))?;

        let alignment = ParagraphAlignment::from_index(stored.alignment)
            .ok_or(ReaderPreferencesError::InvalidAlignment(stored.alignment))?;

        let text = TextSettings::new(stored.font_size, line_spacing, margin)
            .ok_or(ReaderPreferencesError::InvalidFontSize(stored.font_size))?
            .with_alignment(alignment)
            .with_paragraph_indent(stored.paragraph_indent)
            .ok_or(ReaderPreferencesError::InvalidParagraphIndent(
                stored.paragraph_indent,
            ))?
            .with_paragraph_spacing(stored.paragraph_spacing)
            .with_embedded_style(stored.embedded_style)
            .with_hyphenation(
                stored
                    .hyphenation
                    .unwrap_or(TextSettings::default().hyphenation()),
            )
            .with_font(
                stored
                    .font
                    .as_deref()
                    .and_then(card_font)
                    .unwrap_or_default(),
            );

        Ok(Self::new(text))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReaderPreferencesRequest {
    Load,
    Update(ReaderPreferences),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReaderPreferencesError {
    Storage(StorageError),
    InvalidFontSize(u16),
    InvalidLineSpacing(u8),
    InvalidMargin(u8),
    InvalidAlignment(u8),
    InvalidParagraphIndent(u8),
}

impl From<StorageError> for ReaderPreferencesError {
    fn from(error: StorageError) -> Self {
        Self::Storage(error)
    }
}

#[derive(Debug, Encode, Decode)]
struct StoredReaderPreferences {
    #[n(0)]
    font_size: u16,
    #[n(1)]
    line_spacing: u8,
    #[n(2)]
    margin: u8,
    #[n(3)]
    alignment: u8,
    #[n(4)]
    paragraph_indent: u8,
    #[n(5)]
    paragraph_spacing: bool,
    #[n(6)]
    embedded_style: bool,
    /// on when missing, from before the setting existed
    #[n(7)]
    hyphenation: Option<bool>,
    /// the card family's name; the built-in font when missing
    #[n(8)]
    font: Option<String>,
}
