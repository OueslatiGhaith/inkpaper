use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

use super::{READER_FONT_SIZE_DEFAULT, READER_FONT_SIZE_MAX, READER_FONT_SIZE_MIN};

const STORAGE_VERSION: u8 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReaderPreferences {
    font_size: u16,
}

impl ReaderPreferences {
    pub const fn new(font_size: u16) -> Option<Self> {
        if font_size < READER_FONT_SIZE_MIN || font_size > READER_FONT_SIZE_MAX {
            return None;
        }

        Some(Self { font_size })
    }

    pub const fn font_size(self) -> u16 {
        self.font_size
    }

    pub fn encode(self) -> Result<Vec<u8>, ReaderPreferencesError> {
        let stored = StoredReaderPreferences {
            version: STORAGE_VERSION,
            font_size: self.font_size,
        };

        postcard::to_allocvec(&stored).map_err(|_| ReaderPreferencesError::Encode)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, ReaderPreferencesError> {
        let (stored, remainder) = postcard::take_from_bytes::<StoredReaderPreferences>(bytes)
            .map_err(|_| ReaderPreferencesError::Decode)?;

        if !remainder.is_empty() {
            return Err(ReaderPreferencesError::TrailingData);
        }

        if stored.version != STORAGE_VERSION {
            return Err(ReaderPreferencesError::UnsupportedVersion(stored.version));
        }

        Self::new(stored.font_size).ok_or(ReaderPreferencesError::InvalidFontSize(stored.font_size))
    }
}

impl Default for ReaderPreferences {
    fn default() -> Self {
        Self {
            font_size: READER_FONT_SIZE_DEFAULT,
        }
    }
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
    TrailingData,
}

#[derive(Debug, Serialize, Deserialize)]
struct StoredReaderPreferences {
    version: u8,
    font_size: u16,
}
