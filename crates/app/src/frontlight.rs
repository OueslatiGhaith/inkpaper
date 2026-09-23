use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

const STORAGE_VERSION: u8 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrontlightSetting {
    brightness: u8,
    warmth: u8,
    on: bool,
}

impl FrontlightSetting {
    pub const fn new(brightness: u8, warmth: u8, on: bool) -> Option<Self> {
        if brightness == 0 || brightness > 100 || warmth > 100 {
            return None;
        }

        Some(Self {
            brightness,
            warmth,
            on,
        })
    }

    pub const fn brightness(self) -> u8 {
        self.brightness
    }

    pub const fn warmth(self) -> u8 {
        self.warmth
    }

    pub const fn is_on(self) -> bool {
        self.on
    }
}

impl Default for FrontlightSetting {
    fn default() -> Self {
        Self {
            brightness: 25,
            warmth: 50,
            on: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FrontlightPreferences {
    setting: FrontlightSetting,
    restore_on_wake: bool,
}

impl FrontlightPreferences {
    const fn startup_setting(self) -> FrontlightSetting {
        FrontlightSetting {
            brightness: self.setting.brightness,
            warmth: self.setting.warmth,
            on: self.setting.on && self.restore_on_wake,
        }
    }

    pub(crate) fn encode(self) -> Result<Vec<u8>, FrontlightPreferencesError> {
        let stored = StoredFrontlightPreferences {
            version: STORAGE_VERSION,
            brightness: self.setting.brightness,
            warmth: self.setting.warmth,
            on: self.setting.on,
            restore_on_wake: self.restore_on_wake,
        };

        postcard::to_allocvec(&stored).map_err(|_| FrontlightPreferencesError::Encode)
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, FrontlightPreferencesError> {
        let (stored, remainder) = postcard::take_from_bytes::<StoredFrontlightPreferences>(bytes)
            .map_err(|_| FrontlightPreferencesError::Decode)?;

        if !remainder.is_empty() {
            return Err(FrontlightPreferencesError::TrailingData);
        }

        if stored.version != STORAGE_VERSION {
            return Err(FrontlightPreferencesError::UnsupportedVersion(
                stored.version,
            ));
        }

        let setting = FrontlightSetting::new(stored.brightness, stored.warmth, stored.on).ok_or(
            FrontlightPreferencesError::InvalidSetting {
                brightness: stored.brightness,
                warmth: stored.warmth,
            },
        )?;

        Ok(Self {
            setting,
            restore_on_wake: stored.restore_on_wake,
        })
    }
}

impl Default for FrontlightPreferences {
    fn default() -> Self {
        Self {
            setting: FrontlightSetting::default(),
            // restore a light that was on before sleep.
            restore_on_wake: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FrontlightPreferencesRequest {
    Update(FrontlightPreferences),
    Persist,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FrontlightPreferencesError {
    Encode,
    Decode,
    UnsupportedVersion(u8),
    InvalidSetting { brightness: u8, warmth: u8 },
    TrailingData,
}

#[derive(Debug, Serialize, Deserialize)]
struct StoredFrontlightPreferences {
    version: u8,
    brightness: u8,
    warmth: u8,
    on: bool,
    restore_on_wake: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct FrontlightState {
    setting: FrontlightSetting,
    preferences: FrontlightPreferences,
    pending: Option<FrontlightSetting>,
    pending_preferences: Option<FrontlightPreferences>,
    persist_requested: bool,
}

impl FrontlightState {
    pub(crate) const fn setting(&self) -> FrontlightSetting {
        self.setting
    }

    pub(crate) fn apply_preferences(&mut self, preferences: FrontlightPreferences) -> bool {
        let setting = preferences.startup_setting();

        let changed = self.setting != setting || self.preferences != preferences;

        self.setting = setting;
        self.preferences = preferences;
        self.pending = None;
        self.pending_preferences = None;
        self.persist_requested = false;

        changed
    }

    pub(crate) fn request_apply(&mut self) {
        self.pending = Some(self.setting);
    }

    pub(crate) fn take_request(&mut self) -> Option<FrontlightSetting> {
        self.pending.take()
    }

    pub(crate) fn take_preferences_request(&mut self) -> Option<FrontlightPreferencesRequest> {
        if let Some(preferences) = self.pending_preferences.take() {
            return Some(FrontlightPreferencesRequest::Update(preferences));
        }

        if self.persist_requested {
            self.persist_requested = false;
            return Some(FrontlightPreferencesRequest::Persist);
        }

        None
    }

    pub(crate) fn request_persist(&mut self) {
        self.persist_requested = true;
    }

    pub(crate) fn set_brightness(&mut self, brightness: u8) -> bool {
        let brightness = brightness.clamp(1, 100);

        self.replace(FrontlightSetting {
            brightness,
            warmth: self.setting.warmth,
            on: true,
        })
    }

    pub(crate) fn adjust_brightness(&mut self, delta: i16) -> bool {
        let brightness = (i16::from(self.setting.brightness) + delta).clamp(1, 100) as u8;
        self.set_brightness(brightness)
    }

    pub(crate) fn set_warmth(&mut self, warmth: u8) -> bool {
        let warmth = warmth.min(100);

        self.replace(FrontlightSetting {
            brightness: self.setting.brightness,
            warmth,
            on: self.setting.on,
        })
    }

    pub(crate) fn adjust_warmth(&mut self, delta: i16) -> bool {
        let warmth = (i16::from(self.setting.warmth) + delta).clamp(0, 100) as u8;
        self.set_warmth(warmth)
    }

    pub(crate) fn toggle(&mut self) -> bool {
        self.replace(FrontlightSetting {
            brightness: self.setting.brightness,
            warmth: self.setting.warmth,
            on: !self.setting.on,
        })
    }

    fn replace(&mut self, setting: FrontlightSetting) -> bool {
        if self.setting == setting {
            return false;
        }

        self.setting = setting;

        self.preferences.setting = setting;

        self.pending = Some(setting);
        self.pending_preferences = Some(self.preferences);

        true
    }
}

impl Default for FrontlightState {
    fn default() -> Self {
        let preferences = FrontlightPreferences::default();

        Self {
            setting: preferences.startup_setting(),
            preferences,
            pending: None,
            pending_preferences: None,
            persist_requested: false,
        }
    }
}
