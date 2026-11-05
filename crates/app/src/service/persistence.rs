use alloc::{string::String, vec::Vec};

use minicbor::{Decode, Encode};

use crate::{
    ClockPreferences, FrontlightPreferences, FrontlightPreferencesRequest, ReaderPreferences,
    ReadingHistory, SavedNetworks, dictionaries::DictionaryFolder, fonts::FontFamily,
    reader::ReaderFont, storage,
};

use super::{AppPlatform, AppService};

const READING_HISTORY_STATE: &str = "reading-history.dat";
const READER_PREFERENCES_STATE: &str = "reader-preferences.dat";
const FRONTLIGHT_PREFERENCES_STATE: &str = "frontlight-preferences.dat";
const MAX_FRONTLIGHT_PREFERENCES_BYTES: usize = 64;
const CLOCK_PREFERENCES_STATE: &str = "clock-preferences.dat";
const MAX_CLOCK_PREFERENCES_BYTES: usize = 64;
const WIFI_NETWORKS_STATE: &str = "wifi-networks.dat";
const MAX_WIFI_NETWORKS_BYTES: usize = 1024;
const MAX_READING_HISTORY_BYTES: usize = 64 * 1024;
/// room for a font family's and a dictionary's names, each up to FAT's 255
/// bytes
const MAX_READER_PREFERENCES_BYTES: usize = 1024;
const RESUME_STATE: &str = "resume.dat";
/// room for a path of FAT's 255 bytes
const MAX_RESUME_BYTES: usize = 512;

/// The book to reopen at boot.
#[derive(Debug, Encode, Decode)]
struct StoredResume {
    #[n(0)]
    book: Option<String>,
}

impl<P> AppService<P>
where
    P: AppPlatform,
{
    /// The bytes saved as `name`; `None` when nothing was saved or reading
    /// failed.
    pub(super) async fn read_state(&mut self, name: &str, max_bytes: usize) -> Option<Vec<u8>> {
        self.platform
            .load_state(name, max_bytes)
            .await
            .ok()
            .flatten()
    }

    /// Saves `encoded` as `name`, returning whether it was encoded and written.
    pub(super) async fn write_state<E>(&mut self, name: &str, encoded: Result<Vec<u8>, E>) -> bool {
        let Ok(bytes) = encoded else {
            return false;
        };

        self.platform.save_state(name, &bytes).await.is_ok()
    }

    pub(super) async fn load_history(&mut self) -> ReadingHistory {
        self.read_state(READING_HISTORY_STATE, MAX_READING_HISTORY_BYTES)
            .await
            .and_then(|bytes| ReadingHistory::decode(&bytes).ok())
            .unwrap_or_default()
    }

    pub(super) async fn load_preferences(&mut self) -> ReaderPreferences {
        let Some(bytes) = self
            .read_state(READER_PREFERENCES_STATE, MAX_READER_PREFERENCES_BYTES)
            .await
        else {
            return ReaderPreferences::default();
        };

        let families = &self.font_families;
        let dictionaries = &self.dictionaries;

        ReaderPreferences::decode(
            &bytes,
            |name| {
                let index = families.iter().position(|family| family.name() == name)?;

                u8::try_from(index).ok().map(ReaderFont::Card)
            },
            |name| {
                let index = dictionaries
                    .iter()
                    .position(|dictionary| dictionary.name() == name)?;

                u8::try_from(index).ok()
            },
        )
        .unwrap_or_default()
    }

    /// The book to reopen at boot, read once: it's forgotten before it opens,
    /// so a book that brings the device down leaves the next boot on Home.
    /// `None` when it couldn't be forgotten either.
    pub(super) async fn take_resume(&mut self) -> Option<String> {
        let bytes = self.read_state(RESUME_STATE, MAX_RESUME_BYTES).await?;
        let book = storage::decode::<StoredResume>(&bytes).ok()?.book?;

        let forgotten = storage::encode(&StoredResume { book: None });

        self.write_state(RESUME_STATE, forgotten)
            .await
            .then_some(book)
    }

    pub(super) async fn load_frontlight_preferences(&mut self) -> FrontlightPreferences {
        self.read_state(
            FRONTLIGHT_PREFERENCES_STATE,
            MAX_FRONTLIGHT_PREFERENCES_BYTES,
        )
        .await
        .and_then(|bytes| FrontlightPreferences::decode(&bytes).ok())
        .unwrap_or_default()
    }

    pub(super) async fn load_clock_preferences(&mut self) -> ClockPreferences {
        self.read_state(CLOCK_PREFERENCES_STATE, MAX_CLOCK_PREFERENCES_BYTES)
            .await
            .and_then(|bytes| ClockPreferences::decode(&bytes).ok())
            .unwrap_or_default()
    }

    pub(super) async fn load_wifi_networks(&mut self) -> SavedNetworks {
        self.read_state(WIFI_NETWORKS_STATE, MAX_WIFI_NETWORKS_BYTES)
            .await
            .and_then(|bytes| SavedNetworks::decode(&bytes, &self.platform.device_key()).ok())
            .unwrap_or_default()
    }

    /// Networks are saved as soon as one is chosen; they change rarely.
    pub(super) async fn save_wifi_networks(&mut self, networks: &SavedNetworks) -> bool {
        let encoded = networks.encode(&self.platform.device_key());

        self.write_state(WIFI_NETWORKS_STATE, encoded).await
    }

    pub(super) async fn service_frontlight_preferences_request(
        &mut self,
        request: FrontlightPreferencesRequest,
    ) {
        match request {
            FrontlightPreferencesRequest::Update(preferences) => {
                if self.frontlight_preferences == preferences {
                    return;
                }

                self.frontlight_preferences = preferences;
                self.frontlight_preferences_dirty = true;
            }

            FrontlightPreferencesRequest::Persist => {
                self.persist_frontlight_preferences().await;
            }
        }
    }

    pub(super) async fn persist_history(&mut self) -> bool {
        if self.history_dirty {
            let encoded = self.history.encode();
            self.history_dirty = !self.write_state(READING_HISTORY_STATE, encoded).await;
        }

        !self.history_dirty
    }

    pub(super) async fn persist_preferences(&mut self) -> bool {
        if self.preferences_dirty {
            let font_name = match self.preferences.text().font() {
                ReaderFont::Card(index) => self
                    .font_families
                    .get(usize::from(index))
                    .map(FontFamily::name),
                ReaderFont::BuiltIn => None,
            };
            let dictionary_name = self
                .preferences
                .dictionary()
                .and_then(|index| self.dictionaries.get(usize::from(index)))
                .map(DictionaryFolder::name);
            let encoded = self.preferences.encode(font_name, dictionary_name);
            self.preferences_dirty = !self.write_state(READER_PREFERENCES_STATE, encoded).await;
        }

        !self.preferences_dirty
    }

    pub(super) async fn persist_resume(&mut self) -> bool {
        if self.resume_dirty {
            let encoded = storage::encode(&StoredResume {
                book: self.resume.clone(),
            });
            self.resume_dirty = !self.write_state(RESUME_STATE, encoded).await;
        }

        !self.resume_dirty
    }

    pub(super) async fn persist_frontlight_preferences(&mut self) -> bool {
        if self.frontlight_preferences_dirty {
            let encoded = self.frontlight_preferences.encode();
            self.frontlight_preferences_dirty = !self
                .write_state(FRONTLIGHT_PREFERENCES_STATE, encoded)
                .await;
        }

        !self.frontlight_preferences_dirty
    }

    pub(super) async fn persist_clock_preferences(&mut self) -> bool {
        if self.clock_preferences_dirty {
            let encoded = self.clock_preferences.encode();
            self.clock_preferences_dirty =
                !self.write_state(CLOCK_PREFERENCES_STATE, encoded).await;
        }

        !self.clock_preferences_dirty
    }
}
