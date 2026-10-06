use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

use super::{LineSpacing, ScreenMargin, TextSettings};

const STORAGE_VERSION: u8 = 2;

/// version 1 stored only the font size
const STORAGE_VERSION_FONT_SIZE_ONLY: u8 = 1;

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

    pub fn encode(self) -> Result<Vec<u8>, ReaderPreferencesError> {
        let stored = StoredReaderPreferences {
            version: STORAGE_VERSION,
            font_size: self.text.font_size(),
            line_spacing: self.text.line_spacing().index(),
            margin: self.text.margin().px(),
        };

        postcard::to_allocvec(&stored).map_err(|_| ReaderPreferencesError::Encode)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, ReaderPreferencesError> {
        // postcard writes a u8 as one byte, so the version comes first
        let stored = match bytes.first() {
            Some(&STORAGE_VERSION) => take_all::<StoredReaderPreferences>(bytes)?,

            Some(&STORAGE_VERSION_FONT_SIZE_ONLY) => {
                let stored = take_all::<StoredFontSizeOnly>(bytes)?;
                let defaults = TextSettings::default();

                StoredReaderPreferences {
                    version: stored.version,
                    font_size: stored.font_size,
                    line_spacing: defaults.line_spacing().index(),
                    margin: defaults.margin().px(),
                }
            }

            Some(&version) => return Err(ReaderPreferencesError::UnsupportedVersion(version)),

            None => return Err(ReaderPreferencesError::Decode),
        };

        let line_spacing = LineSpacing::from_index(stored.line_spacing).ok_or(
            ReaderPreferencesError::InvalidLineSpacing(stored.line_spacing),
        )?;

        let margin = ScreenMargin::new(stored.margin)
            .ok_or(ReaderPreferencesError::InvalidMargin(stored.margin))?;

        let text = TextSettings::new(stored.font_size, line_spacing, margin)
            .ok_or(ReaderPreferencesError::InvalidFontSize(stored.font_size))?;

        Ok(Self::new(text))
    }
}

fn take_all<'de, T: Deserialize<'de>>(bytes: &'de [u8]) -> Result<T, ReaderPreferencesError> {
    let (stored, remainder) =
        postcard::take_from_bytes::<T>(bytes).map_err(|_| ReaderPreferencesError::Decode)?;

    if !remainder.is_empty() {
        return Err(ReaderPreferencesError::TrailingData);
    }

    Ok(stored)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReaderPreferencesRequest {
    Load,
    Update(ReaderPreferences),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReaderPreferencesError {
    Encode,
    Decode,
    UnsupportedVersion(u8),
    InvalidFontSize(u16),
    InvalidLineSpacing(u8),
    InvalidMargin(u8),
    TrailingData,
}

#[derive(Debug, Serialize, Deserialize)]
struct StoredReaderPreferences {
    version: u8,
    font_size: u16,
    line_spacing: u8,
    margin: u8,
}

#[derive(Debug, Serialize, Deserialize)]
struct StoredFontSizeOnly {
    version: u8,
    font_size: u16,
}
