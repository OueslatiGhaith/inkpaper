use alloc::{format, string::String, vec::Vec};

use serde::{Deserialize, Serialize};

const STORAGE_VERSION: u8 = 1;

/// A fixed offset from UTC in quarter hours, from UTC-12:00 to UTC+14:00.
///
/// Like crosspoint's `clockUtcOffsetQ`, quarter hours cover every offset in use.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UtcOffset {
    quarters: i8,
}

impl UtcOffset {
    const MIN_QUARTERS: i8 = -12 * 4;
    const MAX_QUARTERS: i8 = 14 * 4;
    const SPAN: i32 = (Self::MAX_QUARTERS - Self::MIN_QUARTERS) as i32;

    const fn from_quarters(quarters: i8) -> Option<Self> {
        if quarters < Self::MIN_QUARTERS || quarters > Self::MAX_QUARTERS {
            return None;
        }

        Some(Self { quarters })
    }

    pub(crate) const fn minutes(self) -> i32 {
        self.quarters as i32 * 15
    }

    /// The offset `delta` quarter hours away, clamped to the supported range.
    pub(crate) fn step(self, delta: i8) -> Self {
        let quarters = self
            .quarters
            .saturating_add(delta)
            .clamp(Self::MIN_QUARTERS, Self::MAX_QUARTERS);

        Self { quarters }
    }

    /// Knob position from 0 to 100 on a slider spanning the whole range.
    pub(crate) fn slider_value(self) -> u8 {
        let position = i32::from(self.quarters - Self::MIN_QUARTERS);

        ((position * 100 + Self::SPAN / 2) / Self::SPAN) as u8
    }

    pub(crate) fn from_slider_value(value: u8) -> Self {
        let position = (i32::from(value.min(100)) * Self::SPAN + 50) / 100;

        Self {
            quarters: Self::MIN_QUARTERS + position as i8,
        }
    }

    /// `UTC+05:30`, `UTC-03:30`, `UTC+00:00`
    pub(crate) fn label(self) -> String {
        let sign = if self.quarters < 0 { '-' } else { '+' };
        let minutes = self.minutes().unsigned_abs();

        format!("UTC{}{:02}:{:02}", sign, minutes / 60, minutes % 60)
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ClockPreferences {
    utc_offset: UtcOffset,
}

impl ClockPreferences {
    pub(crate) fn encode(self) -> Result<Vec<u8>, ClockPreferencesError> {
        let stored = StoredClockPreferences {
            version: STORAGE_VERSION,
            utc_offset_quarters: self.utc_offset.quarters,
        };

        postcard::to_allocvec(&stored).map_err(|_| ClockPreferencesError::Encode)
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, ClockPreferencesError> {
        let (stored, remainder) = postcard::take_from_bytes::<StoredClockPreferences>(bytes)
            .map_err(|_| ClockPreferencesError::Decode)?;

        if !remainder.is_empty() {
            return Err(ClockPreferencesError::TrailingData);
        }

        if stored.version != STORAGE_VERSION {
            return Err(ClockPreferencesError::UnsupportedVersion(stored.version));
        }

        let utc_offset = UtcOffset::from_quarters(stored.utc_offset_quarters).ok_or(
            ClockPreferencesError::InvalidOffset(stored.utc_offset_quarters),
        )?;

        Ok(Self { utc_offset })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClockPreferencesError {
    Encode,
    Decode,
    UnsupportedVersion(u8),
    InvalidOffset(i8),
    TrailingData,
}

#[derive(Debug, Serialize, Deserialize)]
struct StoredClockPreferences {
    version: u8,
    utc_offset_quarters: i8,
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct ClockState {
    preferences: ClockPreferences,
    /// changed by a drag and not saved yet
    unsaved: bool,
    pending_save: Option<ClockPreferences>,
}

impl ClockState {
    pub(crate) const fn utc_offset(&self) -> UtcOffset {
        self.preferences.utc_offset
    }

    pub(crate) fn apply_preferences(&mut self, preferences: ClockPreferences) -> bool {
        let changed = self.preferences != preferences;

        self.preferences = preferences;
        self.unsaved = false;
        self.pending_save = None;

        changed
    }

    /// Shows `offset` without saving it; [`Self::commit`] saves it.
    pub(crate) fn preview_utc_offset(&mut self, offset: UtcOffset) -> bool {
        if self.preferences.utc_offset == offset {
            return false;
        }

        self.preferences.utc_offset = offset;
        self.unsaved = true;

        true
    }

    pub(crate) fn set_utc_offset(&mut self, offset: UtcOffset) -> bool {
        let changed = self.preview_utc_offset(offset);
        self.commit();

        changed
    }

    /// Queues a save of a previewed offset. Returns whether there was one.
    pub(crate) fn commit(&mut self) -> bool {
        if !self.unsaved {
            return false;
        }

        self.unsaved = false;
        self.pending_save = Some(self.preferences);

        true
    }

    pub(crate) fn take_save_request(&mut self) -> Option<ClockPreferences> {
        self.pending_save.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offset(quarters: i8) -> UtcOffset {
        UtcOffset::from_quarters(quarters).unwrap()
    }

    #[test]
    fn labels_offsets_with_sign_hours_and_minutes() {
        assert_eq!(offset(0).label(), "UTC+00:00");
        assert_eq!(offset(22).label(), "UTC+05:30");
        assert_eq!(offset(-14).label(), "UTC-03:30");
        assert_eq!(offset(-1).label(), "UTC-00:15");
        assert_eq!(offset(56).label(), "UTC+14:00");
    }

    #[test]
    fn slider_spans_the_whole_range() {
        assert_eq!(UtcOffset::from_slider_value(0).label(), "UTC-12:00");
        assert_eq!(UtcOffset::from_slider_value(100).label(), "UTC+14:00");
        assert_eq!(offset(-48).slider_value(), 0);
        assert_eq!(offset(56).slider_value(), 100);
    }

    #[test]
    fn preferences_round_trip_and_reject_out_of_range_offsets() {
        let preferences = ClockPreferences {
            utc_offset: offset(-14),
        };
        let bytes = preferences.encode().unwrap();

        assert_eq!(ClockPreferences::decode(&bytes), Ok(preferences));

        let stored = StoredClockPreferences {
            version: STORAGE_VERSION,
            utc_offset_quarters: 57,
        };
        let bytes = postcard::to_allocvec(&stored).unwrap();

        assert_eq!(
            ClockPreferences::decode(&bytes),
            Err(ClockPreferencesError::InvalidOffset(57))
        );
    }

    #[test]
    fn a_dragged_offset_is_saved_once_on_commit() {
        let mut clock = ClockState::default();

        clock.preview_utc_offset(offset(4));
        clock.preview_utc_offset(offset(8));
        assert_eq!(clock.take_save_request(), None);

        assert!(clock.commit());
        assert_eq!(
            clock.take_save_request(),
            Some(ClockPreferences {
                utc_offset: offset(8)
            })
        );
        assert!(!clock.commit());
    }
}
