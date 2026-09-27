use alloc::{string::String, vec::Vec};

use serde::{Deserialize, Serialize};

use crate::keyboard::{Key, KeyResult, KeyboardState};

const STORAGE_VERSION: u8 = 1;

/// Like crosspoint, up to eight networks are remembered.
const MAX_SAVED_NETWORKS: usize = 8;

// 802.11 limits: SSIDs are at most 32 bytes, WPA passphrases 8 to 64
const MAX_SSID_BYTES: usize = 32;
const MIN_PASSWORD_BYTES: usize = 8;
const MAX_PASSWORD_BYTES: usize = 64;

/// A network the device can join.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WifiCredentials {
    ssid: String,
    password: String,
}

impl WifiCredentials {
    /// An empty password means an open network.
    pub fn new(ssid: impl Into<String>, password: impl Into<String>) -> Option<Self> {
        let ssid = ssid.into();
        let password = password.into();

        if ssid.is_empty() || ssid.len() > MAX_SSID_BYTES || password.len() > MAX_PASSWORD_BYTES {
            return None;
        }

        Some(Self { ssid, password })
    }

    pub fn ssid(&self) -> &str {
        &self.ssid
    }

    pub fn password(&self) -> &str {
        &self.password
    }
}

/// A network found by a scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WifiNetwork {
    ssid: String,
    rssi: i8,
    secured: bool,
}

impl WifiNetwork {
    pub fn new(ssid: impl Into<String>, rssi: i8, secured: bool) -> Self {
        Self {
            ssid: ssid.into(),
            rssi,
            secured,
        }
    }

    pub(crate) fn ssid(&self) -> &str {
        &self.ssid
    }

    pub(crate) const fn secured(&self) -> bool {
        self.secured
    }
}

/// The scan could not run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WifiScanError;

/// Why joining a network with a new password failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WifiJoinFailure {
    /// the WiFi radio could not start
    Radio,
    /// the network turned the device away or didn't answer, most often
    /// because of a wrong password
    Rejected,
}

impl WifiJoinFailure {
    pub(crate) const fn message(self) -> &'static str {
        match self {
            Self::Radio => "WiFi could not start",
            Self::Rejected => "Could not join. Check the password",
        }
    }
}

/// Remembered networks, most recently chosen first. The first one is the
/// network the device uses.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct SavedNetworks {
    networks: Vec<WifiCredentials>,
}

impl SavedNetworks {
    pub(crate) fn current(&self) -> Option<&WifiCredentials> {
        self.networks.first()
    }

    pub(crate) fn find(&self, ssid: &str) -> Option<&WifiCredentials> {
        self.networks.iter().find(|network| network.ssid == ssid)
    }

    /// Makes `credentials` the network in use. Returns whether anything changed.
    fn remember(&mut self, credentials: WifiCredentials) -> bool {
        if self.current() == Some(&credentials) {
            return false;
        }

        self.networks
            .retain(|network| network.ssid != credentials.ssid);
        self.networks.insert(0, credentials);
        self.networks.truncate(MAX_SAVED_NETWORKS);

        true
    }

    /// Encodes the networks with each password obfuscated by `device_key`.
    pub(crate) fn encode(&self, device_key: &DeviceKey) -> Result<Vec<u8>, SavedNetworksError> {
        let stored = StoredSavedNetworks {
            version: STORAGE_VERSION,
            networks: self
                .networks
                .iter()
                .map(|network| StoredNetwork {
                    ssid: network.ssid.clone(),
                    password: obfuscate(network.password.as_bytes(), device_key),
                })
                .collect(),
        };

        postcard::to_allocvec(&stored).map_err(|_| SavedNetworksError::Encode)
    }

    /// Decodes networks saved with the same `device_key`. Networks that don't
    /// decode, such as from a card moved over from another device, are dropped.
    pub(crate) fn decode(bytes: &[u8], device_key: &DeviceKey) -> Result<Self, SavedNetworksError> {
        let (stored, remainder) = postcard::take_from_bytes::<StoredSavedNetworks>(bytes)
            .map_err(|_| SavedNetworksError::Decode)?;

        if !remainder.is_empty() {
            return Err(SavedNetworksError::TrailingData);
        }

        if stored.version != STORAGE_VERSION {
            return Err(SavedNetworksError::UnsupportedVersion(stored.version));
        }

        let networks = stored
            .networks
            .into_iter()
            .filter_map(|network| {
                let password = String::from_utf8(obfuscate(&network.password, device_key)).ok()?;

                WifiCredentials::new(network.ssid, password)
            })
            .take(MAX_SAVED_NETWORKS)
            .collect();

        Ok(Self { networks })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SavedNetworksError {
    Encode,
    Decode,
    UnsupportedVersion(u8),
    TrailingData,
}

/// Bytes unique to the device, such as its MAC address.
pub type DeviceKey = [u8; 6];

/// Like crosspoint, passwords are XORed with the device key before they reach
/// the SD card. That isn't encryption; it keeps them from being read at a
/// glance over USB file transfer, and ties them to this device. XOR is its own
/// inverse, so this also recovers them.
fn obfuscate(bytes: &[u8], device_key: &DeviceKey) -> Vec<u8> {
    bytes
        .iter()
        .zip(device_key.iter().cycle())
        .map(|(byte, key)| byte ^ key)
        .collect()
}

#[derive(Debug, Serialize, Deserialize)]
struct StoredSavedNetworks {
    version: u8,
    networks: Vec<StoredNetwork>,
}

#[derive(Debug, Serialize, Deserialize)]
struct StoredNetwork {
    ssid: String,
    /// obfuscated with the device key
    password: Vec<u8>,
}

/// Where a typed password is in being tried, like crosspoint, which joins
/// the network before saving it.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JoinStatus {
    #[default]
    Typing,
    Checking,
    Failed(WifiJoinFailure),
}

/// The password being typed for a secured network.
#[derive(Debug)]
pub(crate) struct PasswordEntry {
    ssid: String,
    keyboard: KeyboardState,
    shown: bool,
    status: JoinStatus,
    join_requested: bool,
}

impl PasswordEntry {
    fn new(ssid: String) -> Self {
        Self {
            ssid,
            keyboard: KeyboardState::new(MIN_PASSWORD_BYTES, MAX_PASSWORD_BYTES),
            shown: false,
            status: JoinStatus::Typing,
            join_requested: false,
        }
    }

    pub(crate) const fn status(&self) -> JoinStatus {
        self.status
    }

    /// Presses a key. Submitting tries to join with the password; the keys
    /// wait while that runs. Returns whether anything changed.
    pub(crate) fn press(&mut self, key: Key) -> bool {
        if self.status == JoinStatus::Checking {
            return false;
        }

        match self.keyboard.press(key) {
            KeyResult::Unchanged => false,
            // editing clears a failure
            KeyResult::Changed => {
                self.status = JoinStatus::Typing;
                true
            }
            KeyResult::Submit => {
                self.status = JoinStatus::Checking;
                self.join_requested = true;
                true
            }
        }
    }

    fn take_join_request(&mut self) -> Option<WifiCredentials> {
        if !core::mem::take(&mut self.join_requested) {
            return None;
        }

        self.credentials()
    }

    /// Ends a join. Returns the credentials to save when it worked.
    fn finish_join(&mut self, result: Result<(), WifiJoinFailure>) -> Option<WifiCredentials> {
        if self.status != JoinStatus::Checking {
            return None;
        }

        match result {
            Ok(()) => self.credentials(),
            Err(failure) => {
                self.status = JoinStatus::Failed(failure);
                None
            }
        }
    }

    pub(crate) fn ssid(&self) -> &str {
        &self.ssid
    }

    pub(crate) fn keyboard(&self) -> &KeyboardState {
        &self.keyboard
    }

    /// Whether the password is shown instead of masked.
    pub(crate) const fn shown(&self) -> bool {
        self.shown
    }

    pub(crate) fn toggle_shown(&mut self) {
        self.shown = !self.shown;
    }

    fn credentials(&self) -> Option<WifiCredentials> {
        WifiCredentials::new(self.ssid.clone(), self.keyboard.text())
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WifiScanStatus {
    #[default]
    Idle,
    Scanning,
    Done,
    Failed,
}

#[derive(Debug, Default)]
pub(crate) struct WifiState {
    saved: SavedNetworks,
    save_requested: bool,
    /// strongest first, one entry per name
    networks: Vec<WifiNetwork>,
    scan: WifiScanStatus,
    scan_requested: bool,
    revision: u64,
    password_entry: Option<PasswordEntry>,
}

impl WifiState {
    pub(crate) fn saved(&self) -> &SavedNetworks {
        &self.saved
    }

    pub(crate) fn networks(&self) -> &[WifiNetwork] {
        &self.networks
    }

    pub(crate) const fn scan_status(&self) -> WifiScanStatus {
        self.scan
    }

    /// Changes whenever the scan list is replaced, so the list scrolls back to
    /// the top.
    pub(crate) const fn revision(&self) -> u64 {
        self.revision
    }

    pub(crate) fn apply_saved(&mut self, saved: SavedNetworks) -> bool {
        let changed = self.saved != saved;

        self.saved = saved;
        self.save_requested = false;

        changed
    }

    pub(crate) fn remember(&mut self, credentials: WifiCredentials) -> bool {
        if !self.saved.remember(credentials) {
            return false;
        }

        self.save_requested = true;

        true
    }

    pub(crate) fn begin_password_entry(&mut self, ssid: String) {
        self.password_entry = Some(PasswordEntry::new(ssid));
    }

    pub(crate) fn password_entry(&self) -> Option<&PasswordEntry> {
        self.password_entry.as_ref()
    }

    pub(crate) fn password_entry_mut(&mut self) -> Option<&mut PasswordEntry> {
        self.password_entry.as_mut()
    }

    pub(crate) fn end_password_entry(&mut self) {
        self.password_entry = None;
    }

    pub(crate) fn take_join_request(&mut self) -> Option<WifiCredentials> {
        self.password_entry.as_mut()?.take_join_request()
    }

    /// Ends the password entry's join. Returns the credentials to save when it
    /// worked; a result that comes after the entry closed is dropped.
    pub(crate) fn finish_join(
        &mut self,
        result: Result<(), WifiJoinFailure>,
    ) -> Option<WifiCredentials> {
        self.password_entry.as_mut()?.finish_join(result)
    }

    pub(crate) fn take_save_request(&mut self) -> Option<SavedNetworks> {
        core::mem::take(&mut self.save_requested).then(|| self.saved.clone())
    }

    /// Starts a scan unless one is already running.
    pub(crate) fn request_scan(&mut self) -> bool {
        if self.scan == WifiScanStatus::Scanning {
            return false;
        }

        self.scan = WifiScanStatus::Scanning;
        self.scan_requested = true;

        true
    }

    pub(crate) fn take_scan_request(&mut self) -> bool {
        core::mem::take(&mut self.scan_requested)
    }

    pub(crate) fn finish_scan(&mut self, result: Result<Vec<WifiNetwork>, WifiScanError>) {
        self.revision += 1;

        let Ok(mut found) = result else {
            self.scan = WifiScanStatus::Failed;
            self.networks.clear();
            return;
        };

        // hidden networks have no name to show; access points sharing a name
        // appear once, at their strongest
        found.retain(|network| !network.ssid.is_empty());
        found.sort_by_key(|network| core::cmp::Reverse(network.rssi));

        let mut networks: Vec<WifiNetwork> = Vec::with_capacity(found.len());
        for network in found {
            if !networks.iter().any(|kept| kept.ssid == network.ssid) {
                networks.push(network);
            }
        }

        self.scan = WifiScanStatus::Done;
        self.networks = networks;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn credentials(ssid: &str) -> WifiCredentials {
        WifiCredentials::new(ssid, "password").unwrap()
    }

    #[test]
    fn remembering_moves_a_network_to_the_front_without_duplicates() {
        let mut saved = SavedNetworks::default();

        for index in 0..=MAX_SAVED_NETWORKS {
            saved.remember(credentials(&alloc::format!("net-{index}")));
        }
        assert_eq!(saved.networks.len(), MAX_SAVED_NETWORKS);
        assert!(saved.find("net-0").is_none());

        saved.remember(WifiCredentials::new("net-3", "changed").unwrap());

        assert_eq!(saved.current().unwrap().password(), "changed");
        assert_eq!(saved.networks.len(), MAX_SAVED_NETWORKS);
        assert_eq!(
            saved
                .networks
                .iter()
                .filter(|network| network.ssid() == "net-3")
                .count(),
            1
        );
    }

    #[test]
    fn scan_results_list_each_named_network_once_strongest_first() {
        let mut wifi = WifiState::default();

        wifi.finish_scan(Ok(alloc::vec![
            WifiNetwork::new("office", -70, true),
            WifiNetwork::new("", -30, true),
            WifiNetwork::new("home", -60, true),
            WifiNetwork::new("office", -40, true),
        ]));

        let names: Vec<_> = wifi.networks().iter().map(WifiNetwork::ssid).collect();
        assert_eq!(names, ["office", "home"]);
        assert_eq!(wifi.networks()[0].rssi, -40);
    }

    const KEY: DeviceKey = [0x3c, 0x71, 0xbf, 0x0a, 0x9e, 0x42];

    #[test]
    fn saved_networks_round_trip_with_the_device_key() {
        let mut saved = SavedNetworks::default();
        saved.remember(credentials("home"));
        saved.remember(WifiCredentials::new("cafe", "").unwrap());

        let bytes = saved.encode(&KEY).unwrap();

        assert_eq!(SavedNetworks::decode(&bytes, &KEY), Ok(saved));
    }

    #[test]
    fn passwords_are_not_stored_readably() {
        let mut saved = SavedNetworks::default();
        saved.remember(WifiCredentials::new("home", "correct horse battery").unwrap());

        let bytes = saved.encode(&KEY).unwrap();

        assert!(
            !bytes
                .windows(b"correct horse".len())
                .any(|window| window == b"correct horse")
        );

        let other_device = [0x10, 0x20, 0x30, 0x40, 0x50, 0x60];
        let decoded = SavedNetworks::decode(&bytes, &other_device).unwrap();
        assert!(
            decoded
                .current()
                .is_none_or(|network| network.password() != "correct horse battery")
        );
    }

    fn type_password(wifi: &mut WifiState, password: &str) {
        let entry = wifi.password_entry_mut().unwrap();

        for byte in password.bytes() {
            entry.press(Key::Char(byte));
        }

        entry.press(Key::Submit);
    }

    #[test]
    fn a_typed_password_is_saved_only_after_joining_works() {
        let mut wifi = WifiState::default();
        wifi.begin_password_entry(String::from("home"));
        type_password(&mut wifi, "password");

        let request = wifi.take_join_request().unwrap();
        assert_eq!(request.password(), "password");
        assert_eq!(wifi.take_join_request(), None);

        // the keys wait while the join runs
        let entry = wifi.password_entry_mut().unwrap();
        assert!(!entry.press(Key::Delete));
        assert_eq!(entry.status(), JoinStatus::Checking);

        assert_eq!(wifi.finish_join(Ok(())), Some(request));
    }

    #[test]
    fn a_failed_join_keeps_the_password_to_fix() {
        let mut wifi = WifiState::default();
        wifi.begin_password_entry(String::from("home"));
        type_password(&mut wifi, "password");
        wifi.take_join_request();

        assert_eq!(wifi.finish_join(Err(WifiJoinFailure::Rejected)), None);

        let entry = wifi.password_entry_mut().unwrap();
        assert_eq!(
            entry.status(),
            JoinStatus::Failed(WifiJoinFailure::Rejected)
        );

        assert!(entry.press(Key::Char(b's')));
        assert_eq!(entry.status(), JoinStatus::Typing);
        assert_eq!(entry.keyboard().text(), "passwords");
    }

    #[test]
    fn a_join_result_after_the_entry_closed_is_dropped() {
        let mut wifi = WifiState::default();
        wifi.begin_password_entry(String::from("home"));
        type_password(&mut wifi, "password");
        wifi.take_join_request();
        wifi.end_password_entry();

        assert_eq!(wifi.finish_join(Ok(())), None);
        assert!(wifi.saved().current().is_none());
    }
}
