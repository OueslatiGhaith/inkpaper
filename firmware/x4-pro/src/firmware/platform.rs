use alloc::vec::Vec;

use inkpaper_app::{
    AppPlatform, DeviceKey, FrontlightSetting, PlatformEntry, WifiCredentials, WifiJoinPlan,
    WifiJoinToken,
};
use inkpaper_io::ByteSource;

use crate::firmware::{
    frontlight::Frontlight,
    storage::{self, RandomAccessHandle, StorageError},
    wifi,
};

pub(super) struct X4Platform {
    frontlight: Frontlight,
}

impl X4Platform {
    pub(super) const fn new(frontlight: Frontlight) -> Self {
        Self { frontlight }
    }

    pub(super) fn frontlight_off(&mut self) {
        self.frontlight.off();
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct X4RandomAccessSource {
    handle: u32,
    len: u64,
}

impl X4RandomAccessSource {
    const fn from_handle(handle: RandomAccessHandle) -> Self {
        Self {
            handle: handle.id(),
            len: handle.len(),
        }
    }
}

impl ByteSource for X4RandomAccessSource {
    type Error = StorageError;

    async fn len(&mut self) -> Result<u64, Self::Error> {
        Ok(self.len)
    }

    async fn read_exact_at(&mut self, offset: u64, buffer: &mut [u8]) -> Result<(), Self::Error> {
        storage::read_random_access_into(self.handle, offset, buffer).await
    }
}

impl AppPlatform for X4Platform {
    type Error = StorageError;
    type RandomAccessSource = X4RandomAccessSource;

    async fn list_directory(&mut self, path: &str) -> Result<Vec<PlatformEntry>, Self::Error> {
        let entries = storage::list_directory(path).await?;

        Ok(entries
            .into_iter()
            .map(|entry| {
                let (name, is_directory) = entry.into_parts();

                if is_directory {
                    PlatformEntry::directory(name)
                } else {
                    PlatformEntry::file(name)
                }
            })
            .collect())
    }

    async fn open_random_access(
        &mut self,
        path: &str,
    ) -> Result<Self::RandomAccessSource, Self::Error> {
        let handle = storage::open_random_access(path).await?;

        Ok(X4RandomAccessSource::from_handle(handle))
    }

    async fn read_file(&mut self, path: &str, max_bytes: usize) -> Result<Vec<u8>, Self::Error> {
        storage::read_file(path, max_bytes).await
    }

    async fn set_frontlight(&mut self, setting: FrontlightSetting) -> Result<(), Self::Error> {
        let brightness = if setting.is_on() {
            setting.brightness()
        } else {
            0
        };

        let brightness = ::frontlight::Percent::new(brightness)
            .expect("app frontlight brightness must be valid");
        let warmth = ::frontlight::Percent::new(setting.warmth())
            .expect("app frontlight warmth must be valid");

        self.frontlight
            .set(::frontlight::Setting::new(brightness, warmth));

        Ok(())
    }

    async fn load_state(
        &mut self,
        name: &str,
        max_bytes: usize,
    ) -> Result<Option<Vec<u8>>, Self::Error> {
        storage::load_state(name, max_bytes).await
    }

    async fn save_state(&mut self, name: &str, bytes: &[u8]) -> Result<(), Self::Error> {
        storage::save_state(name, bytes).await
    }

    async fn start_clock_sync(&mut self, plan: &WifiJoinPlan) -> Result<(), Self::Error> {
        wifi::request_clock_sync(plan.clone());

        Ok(())
    }

    /// The factory MAC address, like crosspoint's password obfuscation key.
    fn device_key(&self) -> DeviceKey {
        let mut key = DeviceKey::default();
        key.copy_from_slice(esp_hal::efuse::base_mac_address().as_bytes());
        key
    }

    async fn start_wifi_join(
        &mut self,
        token: WifiJoinToken,
        network: &WifiCredentials,
    ) -> Result<(), Self::Error> {
        wifi::request_join(token, network.clone());

        Ok(())
    }

    async fn start_wifi_scan(&mut self) -> Result<(), Self::Error> {
        wifi::request_scan();

        Ok(())
    }

    async fn enter_usb_drive(&mut self) -> Result<(), Self::Error> {
        if storage::enter_usb_drive_and_wait().await {
            Ok(())
        } else {
            Err(StorageError::Unavailable)
        }
    }
}
