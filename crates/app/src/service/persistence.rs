use alloc::{string::String, vec::Vec};

use minicbor::{Decode, Encode};

use crate::{
    ClockPreferences, FrontlightPreferences, FrontlightPreferencesRequest, ReaderPreferences,
    ReadingHistory, SavedNetworks, dictionaries::DictionaryFolder, fonts::FontFamily,
    reader::ReaderFont, resume::SleptBook, storage,
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

/// Application state mirrored to a persisted `.dat` file.
#[derive(Debug, Default)]
pub(super) struct Persisted<T> {
    value: T,
    dirty: bool,
}

impl<T> Persisted<T> {
    pub(super) fn get(&self) -> &T {
        &self.value
    }

    pub(super) fn get_mut(&mut self) -> &mut T {
        &mut self.value
    }

    /// Replaces the value with one loaded from storage (not dirty).
    pub(super) fn load(&mut self, value: T) {
        self.value = value;
        self.dirty = false;
    }

    /// Replaces the value and marks it dirty for persistence.
    pub(super) fn set(&mut self, value: T) {
        self.value = value;
        self.dirty = true;
    }

    /// Marks the current value dirty after an in-place mutation.
    pub(super) fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    pub(super) const fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub(super) fn mark_clean(&mut self) {
        self.dirty = false;
    }
}

/// The book to reopen at boot, and when the device went to sleep.
#[derive(Encode)]
struct EncodedResume<'a> {
    #[n(0)]
    book: Option<&'a str>,
    /// minutes since 1970, when the clock was set
    #[n(1)]
    slept_at: Option<u32>,
}

#[derive(Debug, Decode)]
struct StoredResume {
    #[n(0)]
    book: Option<String>,
    /// minutes since 1970, when the clock was set
    #[n(1)]
    slept_at: Option<u32>,
}

#[cfg(feature = "defmt")]
fn warn_storage(action: &str, name: &str) {
    defmt::warn!("storage {} failed for {}", action, name);
}

#[cfg(not(feature = "defmt"))]
fn warn_storage(_action: &str, _name: &str) {}

async fn write_platform_state<P, E>(
    platform: &mut P,
    name: &str,
    encoded: Result<Vec<u8>, E>,
) -> bool
where
    P: AppPlatform,
{
    let Ok(bytes) = encoded else {
        warn_storage("encode", name);
        return false;
    };

    if platform.save_state(name, &bytes).await.is_err() {
        warn_storage("write", name);
        return false;
    }

    true
}

async fn persist_slot<P, T, E>(
    platform: &mut P,
    name: &str,
    slot: &mut Persisted<T>,
    encode: impl FnOnce(&T) -> Result<Vec<u8>, E>,
) -> bool
where
    P: AppPlatform,
{
    if !slot.is_dirty() {
        return true;
    }

    if write_platform_state(platform, name, encode(slot.get())).await {
        slot.mark_clean();
        true
    } else {
        false
    }
}

impl<P> AppService<P>
where
    P: AppPlatform,
{
    /// The bytes saved as `name`; `None` when nothing was saved or reading
    /// failed.
    pub(super) async fn read_state(&mut self, name: &str, max_bytes: usize) -> Option<Vec<u8>> {
        match self.platform.load_state(name, max_bytes).await {
            Ok(bytes) => bytes,
            Err(_) => {
                warn_storage("read", name);
                None
            }
        }
    }

    /// Saves `encoded` as `name`, returning whether it was encoded and written.
    pub(super) async fn write_state<E>(&mut self, name: &str, encoded: Result<Vec<u8>, E>) -> bool {
        write_platform_state(&mut self.platform, name, encoded).await
    }

    async fn load_decoded<T, E>(
        &mut self,
        name: &str,
        max_bytes: usize,
        decode: impl FnOnce(&[u8]) -> Result<T, E>,
    ) -> T
    where
        T: Default,
    {
        let Some(bytes) = self.read_state(name, max_bytes).await else {
            return T::default();
        };

        match decode(&bytes) {
            Ok(value) => value,
            Err(_) => {
                warn_storage("decode", name);
                T::default()
            }
        }
    }

    pub(super) async fn load_history(&mut self) -> ReadingHistory {
        self.load_decoded(
            READING_HISTORY_STATE,
            MAX_READING_HISTORY_BYTES,
            ReadingHistory::decode,
        )
        .await
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

        match ReaderPreferences::decode(
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
        ) {
            Ok(preferences) => preferences,
            Err(_) => {
                warn_storage("decode", READER_PREFERENCES_STATE);
                ReaderPreferences::default()
            }
        }
    }

    /// The book to reopen at boot, read once: it's forgotten before it opens,
    /// so a book that brings the device down leaves the next boot on Home.
    /// `None` when it couldn't be forgotten either.
    pub(super) async fn take_resume(&mut self) -> Option<SleptBook> {
        let bytes = self.read_state(RESUME_STATE, MAX_RESUME_BYTES).await?;
        let stored = match storage::decode::<StoredResume>(&bytes) {
            Ok(stored) => stored,
            Err(_) => {
                warn_storage("decode", RESUME_STATE);
                return None;
            }
        };
        let book = SleptBook {
            path: stored.book?,
            slept_at: stored.slept_at,
        };

        let forgotten = storage::encode(&EncodedResume {
            book: None,
            slept_at: None,
        });

        self.write_state(RESUME_STATE, forgotten)
            .await
            .then_some(book)
    }

    pub(super) async fn load_frontlight_preferences(&mut self) -> FrontlightPreferences {
        self.load_decoded(
            FRONTLIGHT_PREFERENCES_STATE,
            MAX_FRONTLIGHT_PREFERENCES_BYTES,
            FrontlightPreferences::decode,
        )
        .await
    }

    pub(super) async fn load_clock_preferences(&mut self) -> ClockPreferences {
        self.load_decoded(
            CLOCK_PREFERENCES_STATE,
            MAX_CLOCK_PREFERENCES_BYTES,
            ClockPreferences::decode,
        )
        .await
    }

    pub(super) async fn load_wifi_networks(&mut self) -> SavedNetworks {
        let key = self.platform.device_key();
        self.load_decoded(WIFI_NETWORKS_STATE, MAX_WIFI_NETWORKS_BYTES, |bytes| {
            SavedNetworks::decode(bytes, &key)
        })
        .await
    }

    pub(super) async fn service_frontlight_preferences_request(
        &mut self,
        request: FrontlightPreferencesRequest,
    ) {
        match request {
            FrontlightPreferencesRequest::Update(preferences) => {
                if *self.frontlight_preferences.get() == preferences {
                    return;
                }

                self.frontlight_preferences.set(preferences);
            }

            FrontlightPreferencesRequest::Persist => {
                self.persist_frontlight_preferences().await;
            }
        }
    }

    pub(super) async fn persist_history(&mut self) -> bool {
        persist_slot(
            &mut self.platform,
            READING_HISTORY_STATE,
            &mut self.history,
            ReadingHistory::encode,
        )
        .await
    }

    pub(super) async fn persist_preferences(&mut self) -> bool {
        let font_families = &self.font_families;
        let dictionaries = &self.dictionaries;

        persist_slot(
            &mut self.platform,
            READER_PREFERENCES_STATE,
            &mut self.preferences,
            |preferences| {
                let font_name = match preferences.text().font() {
                    ReaderFont::Card(index) => font_families
                        .get(usize::from(index))
                        .map(FontFamily::name),
                    ReaderFont::BuiltIn => None,
                };
                let dictionary_name = preferences
                    .dictionary()
                    .and_then(|index| dictionaries.get(usize::from(index)))
                    .map(DictionaryFolder::name);

                preferences.encode(font_name, dictionary_name)
            },
        )
        .await
    }

    pub(super) async fn persist_resume(&mut self) -> bool {
        persist_slot(
            &mut self.platform,
            RESUME_STATE,
            &mut self.resume,
            |resume| {
                storage::encode(&EncodedResume {
                    book: resume.as_ref().map(|book| book.path.as_str()),
                    slept_at: resume.as_ref().and_then(|book| book.slept_at),
                })
            },
        )
        .await
    }

    pub(super) async fn persist_frontlight_preferences(&mut self) -> bool {
        persist_slot(
            &mut self.platform,
            FRONTLIGHT_PREFERENCES_STATE,
            &mut self.frontlight_preferences,
            |preferences| preferences.encode(),
        )
        .await
    }

    pub(super) async fn persist_clock_preferences(&mut self) -> bool {
        persist_slot(
            &mut self.platform,
            CLOCK_PREFERENCES_STATE,
            &mut self.clock_preferences,
            |preferences| preferences.encode(),
        )
        .await
    }

    pub(super) async fn persist_wifi_networks(&mut self) -> bool {
        let key = self.platform.device_key();
        persist_slot(
            &mut self.platform,
            WIFI_NETWORKS_STATE,
            &mut self.wifi_networks,
            |networks| networks.encode(&key),
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use alloc::{collections::BTreeMap, string::String, vec::Vec};
    use futures_lite::future;
    use inkpaper_io::SliceSource;

    use crate::{
        AppPlatform, DeviceKey, FrontlightSetting, PlatformEntry, WifiCredentials, WifiJoinPlan,
        WifiJoinToken,
    };

    use super::*;

    struct MemoryPlatform {
        files: BTreeMap<String, Vec<u8>>,
        fail_saves: bool,
    }

    impl MemoryPlatform {
        fn new() -> Self {
            Self {
                files: BTreeMap::new(),
                fail_saves: false,
            }
        }
    }

    impl AppPlatform for MemoryPlatform {
        type Error = ();
        type RandomAccessSource = SliceSource<'static>;

        async fn list_directory(&mut self, _: &str) -> Result<Vec<PlatformEntry>, Self::Error> {
            Ok(Vec::new())
        }

        async fn open_random_access(
            &mut self,
            _: &str,
        ) -> Result<Self::RandomAccessSource, Self::Error> {
            Err(())
        }

        async fn read_file(&mut self, _: &str, _: usize) -> Result<Vec<u8>, Self::Error> {
            Err(())
        }

        async fn set_frontlight(&mut self, _: FrontlightSetting) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn load_state(
            &mut self,
            name: &str,
            max_bytes: usize,
        ) -> Result<Option<Vec<u8>>, Self::Error> {
            let Some(bytes) = self.files.get(name) else {
                return Ok(None);
            };
            if bytes.len() > max_bytes {
                return Err(());
            }
            Ok(Some(bytes.clone()))
        }

        async fn save_state(&mut self, name: &str, bytes: &[u8]) -> Result<(), Self::Error> {
            if self.fail_saves {
                return Err(());
            }
            self.files.insert(String::from(name), bytes.to_vec());
            Ok(())
        }

        async fn enter_usb_drive(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }

        fn device_key(&self) -> DeviceKey {
            [0x3c, 0x71, 0xbf, 0x0a, 0x9e, 0x42]
        }

        async fn start_clock_sync(&mut self, _: &WifiJoinPlan) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn start_wifi_join(
            &mut self,
            _: WifiJoinToken,
            _: &WifiCredentials,
        ) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn start_wifi_scan(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }
    }

    #[test]
    fn flush_retries_dirty_wifi_networks_after_transient_save_failure() {
        let mut service = AppService::new(MemoryPlatform::new());
        let mut wifi = crate::wifi::WifiState::default();
        assert!(wifi.connect(WifiCredentials::new("home", "secretpass").unwrap()));
        let saved = wifi.take_save_request().unwrap();

        service.platform.fail_saves = true;
        service.wifi_networks.set(saved.clone());
        assert!(!future::block_on(service.persist_wifi_networks()));
        assert!(service.wifi_networks.is_dirty());
        assert!(!future::block_on(service.flush()));

        // Once storage is healthy again, flush persists WiFi networks too.
        service.platform.fail_saves = false;
        assert!(future::block_on(service.flush()));
        assert!(!service.wifi_networks.is_dirty());

        let reloaded = future::block_on(service.load_wifi_networks());
        assert_eq!(reloaded, saved);
    }
}
