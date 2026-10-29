use alloc::{string::String, vec::Vec};

use inkpaper_epub::EpubSource;

use crate::{BrowseEntry, DeviceKey, FrontlightSetting, WifiCredentials, WifiJoinPlan};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlatformEntryKind {
    Directory,
    File,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformEntry {
    name: String,
    kind: PlatformEntryKind,
}

impl PlatformEntry {
    pub fn directory(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            kind: PlatformEntryKind::Directory,
        }
    }

    pub fn file(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            kind: PlatformEntryKind::File,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub const fn is_directory(&self) -> bool {
        matches!(self.kind, PlatformEntryKind::Directory)
    }

    pub(super) fn into_browse_entry(self) -> BrowseEntry {
        match self.kind {
            PlatformEntryKind::Directory => BrowseEntry::directory(self.name),
            PlatformEntryKind::File => BrowseEntry::file(self.name),
        }
    }
}

#[allow(async_fn_in_trait)]
pub trait AppPlatform {
    type Error;
    type RandomAccessSource: EpubSource;

    async fn list_directory(&mut self, path: &str) -> Result<Vec<PlatformEntry>, Self::Error>;

    /// Opens `path` to read anywhere in it. At least four files stay open
    /// together, a book and a dictionary's; opening more may close the one
    /// read least recently.
    async fn open_random_access(
        &mut self,
        path: &str,
    ) -> Result<Self::RandomAccessSource, Self::Error>;

    /// Reads the whole file at `path`, refusing one over `max_bytes`.
    async fn read_file(&mut self, path: &str, max_bytes: usize) -> Result<Vec<u8>, Self::Error>;

    async fn set_frontlight(&mut self, setting: FrontlightSetting) -> Result<(), Self::Error>;

    async fn load_state(
        &mut self,
        name: &str,
        max_bytes: usize,
    ) -> Result<Option<Vec<u8>>, Self::Error>;

    async fn save_state(&mut self, name: &str, bytes: &[u8]) -> Result<(), Self::Error>;

    async fn enter_usb_drive(&mut self) -> Result<(), Self::Error>;

    /// Bytes unique to this device, used to obfuscate saved WiFi passwords.
    fn device_key(&self) -> DeviceKey;

    /// Starts setting the clock over a network from `plan` and returns without
    /// waiting. The connected network comes first; when it can't be joined,
    /// the platform scans and tries the fallbacks in range. It reports the
    /// outcome through [`InkPaperApp::apply_clock_sync_result`].
    async fn start_clock_sync(&mut self, plan: &WifiJoinPlan) -> Result<(), Self::Error>;

    /// Starts joining `network` to check its password, then leaves it, and
    /// returns without waiting. The platform reports the outcome through
    /// [`InkPaperApp::apply_wifi_join_result`].
    async fn start_wifi_join(&mut self, network: &WifiCredentials) -> Result<(), Self::Error>;

    /// Starts a WiFi scan and returns without waiting. The platform reports the
    /// networks through [`InkPaperApp::apply_wifi_scan_result`].
    async fn start_wifi_scan(&mut self) -> Result<(), Self::Error>;
}
