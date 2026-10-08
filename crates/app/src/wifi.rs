use alloc::{string::String, vec::Vec};

use minicbor::{Decode, Encode};

use crate::keyboard::{Key, KeyResult, KeyboardState};
use crate::paging::{PageTurn, Pager};
use crate::storage::{self, StorageError};

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

/// A remembered network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SavedNetwork {
    credentials: WifiCredentials,
    /// whether a connection may fall back to it when the connected network
    /// can't be joined
    auto_connect: bool,
}

impl SavedNetwork {
    pub(crate) fn ssid(&self) -> &str {
        self.credentials.ssid()
    }

    pub(crate) const fn credentials(&self) -> &WifiCredentials {
        &self.credentials
    }

    pub(crate) const fn auto_connect(&self) -> bool {
        self.auto_connect
    }
}

/// Remembered networks, most recently connected first. The first one is the
/// connected network, the one the device joins when it needs WiFi, unless it
/// was disconnected.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct SavedNetworks {
    networks: Vec<SavedNetwork>,
    connected: bool,
}

impl SavedNetworks {
    pub(crate) fn connected(&self) -> Option<&WifiCredentials> {
        self.networks
            .first()
            .filter(|_| self.connected)
            .map(|network| &network.credentials)
    }

    pub(crate) fn find(&self, ssid: &str) -> Option<&SavedNetwork> {
        self.networks.iter().find(|network| network.ssid() == ssid)
    }

    /// Makes `credentials` the connected network, keeping its auto-connect
    /// setting if it was saved. Returns whether anything changed.
    fn connect(&mut self, credentials: WifiCredentials) -> bool {
        if self.connected() == Some(&credentials) {
            return false;
        }

        let auto_connect = self
            .find(credentials.ssid())
            .is_none_or(SavedNetwork::auto_connect);

        self.networks
            .retain(|network| network.ssid() != credentials.ssid());
        self.networks.insert(
            0,
            SavedNetwork {
                credentials,
                auto_connect,
            },
        );
        self.networks.truncate(MAX_SAVED_NETWORKS);
        self.connected = true;

        true
    }

    fn disconnect(&mut self) -> bool {
        core::mem::take(&mut self.connected)
    }

    /// Drops a network and its password.
    fn forget(&mut self, ssid: &str) -> bool {
        let Some(index) = self
            .networks
            .iter()
            .position(|network| network.ssid() == ssid)
        else {
            return false;
        };

        self.networks.remove(index);

        if index == 0 {
            self.connected = false;
        }

        true
    }

    fn set_auto_connect(&mut self, ssid: &str, auto_connect: bool) -> bool {
        let Some(network) = self
            .networks
            .iter_mut()
            .find(|network| network.ssid() == ssid)
        else {
            return false;
        };

        let changed = network.auto_connect != auto_connect;
        network.auto_connect = auto_connect;

        changed
    }

    /// The networks a connection may use, or nothing when there are none.
    pub(crate) fn join_plan(&self) -> Option<WifiJoinPlan> {
        let connected = self.connected().cloned();

        let fallbacks: Vec<_> = self
            .networks
            .iter()
            .filter(|network| network.auto_connect)
            .map(|network| network.credentials.clone())
            .filter(|credentials| Some(credentials) != connected.as_ref())
            .collect();

        if connected.is_none() && fallbacks.is_empty() {
            return None;
        }

        Some(WifiJoinPlan {
            connected,
            fallbacks,
        })
    }

    /// Encodes the networks with each password obfuscated by `device_key`.
    pub(crate) fn encode(&self, device_key: &DeviceKey) -> Result<Vec<u8>, SavedNetworksError> {
        let stored = StoredSavedNetworks {
            connected: self.connected,
            networks: self
                .networks
                .iter()
                .map(|network| StoredNetwork {
                    ssid: network.credentials.ssid.clone(),
                    password: obfuscate(network.credentials.password.as_bytes(), device_key),
                    auto_connect: network.auto_connect,
                })
                .collect(),
        };

        Ok(storage::encode(&stored)?)
    }

    /// Decodes networks saved with the same `device_key`. Networks that don't
    /// decode, such as from a card moved over from another device, are dropped.
    pub(crate) fn decode(bytes: &[u8], device_key: &DeviceKey) -> Result<Self, SavedNetworksError> {
        let stored: StoredSavedNetworks = storage::decode(bytes)?;

        let mut connected = stored.connected;
        let mut networks = Vec::new();

        for (index, network) in stored.networks.into_iter().enumerate() {
            let credentials = String::from_utf8(obfuscate(&network.password, device_key))
                .ok()
                .and_then(|password| WifiCredentials::new(network.ssid, password));

            match credentials {
                Some(credentials) if networks.len() < MAX_SAVED_NETWORKS => {
                    networks.push(SavedNetwork {
                        credentials,
                        auto_connect: network.auto_connect,
                    });
                }
                // a connected network that didn't decode can't stay connected
                _ if index == 0 => connected = false,
                _ => {}
            }
        }

        Ok(Self {
            networks,
            connected,
        })
    }
}

/// The networks a connection may use: the connected network first, then saved
/// networks with auto-connect on, which are only worth trying when a scan
/// finds them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WifiJoinPlan {
    connected: Option<WifiCredentials>,
    fallbacks: Vec<WifiCredentials>,
}

impl WifiJoinPlan {
    pub fn connected(&self) -> Option<&WifiCredentials> {
        self.connected.as_ref()
    }

    pub fn has_fallbacks(&self) -> bool {
        !self.fallbacks.is_empty()
    }

    /// The fallback networks a scan found, strongest first.
    pub fn fallbacks_in_range(&self, found: &[WifiNetwork]) -> Vec<&WifiCredentials> {
        let mut in_range: Vec<_> = self
            .fallbacks
            .iter()
            .filter_map(|credentials| {
                let strongest = found
                    .iter()
                    .filter(|network| network.ssid == credentials.ssid)
                    .map(|network| network.rssi)
                    .max()?;

                Some((credentials, strongest))
            })
            .collect();

        in_range.sort_by_key(|(_, rssi)| core::cmp::Reverse(*rssi));

        in_range
            .into_iter()
            .map(|(credentials, _)| credentials)
            .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SavedNetworksError {
    Storage(StorageError),
}

impl From<StorageError> for SavedNetworksError {
    fn from(error: StorageError) -> Self {
        Self::Storage(error)
    }
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

#[derive(Debug, Encode, Decode)]
struct StoredSavedNetworks {
    #[n(0)]
    connected: bool,
    #[n(1)]
    networks: Vec<StoredNetwork>,
}

#[derive(Debug, Encode, Decode)]
struct StoredNetwork {
    #[n(0)]
    ssid: String,
    /// obfuscated with the device key
    #[cbor(n(1), with = "minicbor::bytes")]
    password: Vec<u8>,
    #[n(2)]
    auto_connect: bool,
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
    pager: Pager,
    password_entry: Option<PasswordEntry>,
    menu: Option<NetworkMenu>,
}

/// The long-press menu of a saved network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NetworkMenu {
    ssid: String,
    top: i32,
}

impl NetworkMenu {
    pub(crate) fn ssid(&self) -> &str {
        &self.ssid
    }

    pub(crate) const fn top(&self) -> i32 {
        self.top
    }
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

    /// Changes whenever the scan list is replaced.
    pub(crate) const fn revision(&self) -> u64 {
        self.revision
    }

    /// The page of networks shown when `rows_per_page` fit on one.
    pub(crate) fn page(&self, rows_per_page: usize) -> usize {
        self.pager.page(self.networks.len(), rows_per_page)
    }

    pub(crate) fn page_count(&self, rows_per_page: usize) -> usize {
        Pager::page_count(self.networks.len(), rows_per_page)
    }

    pub(crate) fn turn_page(&mut self, turn: PageTurn, rows_per_page: usize) -> bool {
        self.pager.turn(turn, self.networks.len(), rows_per_page)
    }

    pub(crate) fn apply_saved(&mut self, saved: SavedNetworks) -> bool {
        let changed = self.saved != saved;

        self.saved = saved;
        self.save_requested = false;

        changed
    }

    /// Applies a change to the saved networks, saving them if it changed
    /// anything.
    fn change_saved(&mut self, change: impl FnOnce(&mut SavedNetworks) -> bool) -> bool {
        let changed = change(&mut self.saved);
        self.save_requested |= changed;

        changed
    }

    pub(crate) fn connect(&mut self, credentials: WifiCredentials) -> bool {
        self.change_saved(|saved| saved.connect(credentials))
    }

    pub(crate) fn disconnect(&mut self) -> bool {
        self.change_saved(SavedNetworks::disconnect)
    }

    pub(crate) fn forget(&mut self, ssid: &str) -> bool {
        self.change_saved(|saved| saved.forget(ssid))
    }

    pub(crate) fn toggle_auto_connect(&mut self, ssid: &str) -> bool {
        let Some(auto_connect) = self.saved.find(ssid).map(SavedNetwork::auto_connect) else {
            return false;
        };

        self.change_saved(|saved| saved.set_auto_connect(ssid, !auto_connect))
    }

    /// Opens the menu of a saved network, `top` px down the screen.
    pub(crate) fn open_menu(&mut self, ssid: &str, top: i32) -> bool {
        if self.saved.find(ssid).is_none() {
            return false;
        }

        self.menu = Some(NetworkMenu {
            ssid: String::from(ssid),
            top,
        });

        true
    }

    pub(crate) fn menu(&self) -> Option<&NetworkMenu> {
        self.menu.as_ref()
    }

    pub(crate) fn close_menu(&mut self) -> bool {
        self.menu.take().is_some()
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

    /// Replaces the list, back on its first page.
    pub(crate) fn finish_scan(&mut self, result: Result<Vec<WifiNetwork>, WifiScanError>) {
        self.revision += 1;
        self.pager = Pager::default();

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
    fn connecting_moves_a_network_to_the_front_without_duplicates() {
        let mut saved = SavedNetworks::default();

        for index in 0..=MAX_SAVED_NETWORKS {
            saved.connect(credentials(&alloc::format!("net-{index}")));
        }
        assert_eq!(saved.networks.len(), MAX_SAVED_NETWORKS);
        assert!(saved.find("net-0").is_none());

        saved.connect(WifiCredentials::new("net-3", "changed").unwrap());

        assert_eq!(saved.connected().unwrap().password(), "changed");
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
    fn disconnecting_or_forgetting_leaves_no_network_connected() {
        let mut saved = SavedNetworks::default();
        saved.connect(credentials("home"));
        saved.connect(credentials("office"));

        assert!(saved.disconnect());
        assert_eq!(saved.connected(), None);
        assert!(saved.find("office").is_some());

        saved.connect(credentials("home"));
        assert!(saved.forget("home"));
        assert_eq!(saved.connected(), None);
        assert!(saved.find("home").is_none());

        // forgetting another network keeps the connected one
        saved.connect(credentials("home"));
        saved.forget("office");
        assert_eq!(saved.connected().map(WifiCredentials::ssid), Some("home"));
    }

    #[test]
    fn a_new_password_keeps_the_auto_connect_setting() {
        let mut saved = SavedNetworks::default();
        saved.connect(credentials("home"));
        saved.set_auto_connect("home", false);

        saved.connect(WifiCredentials::new("home", "new password").unwrap());

        let home = saved.find("home").unwrap();
        assert_eq!(home.credentials.password(), "new password");
        assert!(!home.auto_connect());
    }

    #[test]
    fn a_join_plan_falls_back_to_auto_connect_networks_in_range() {
        let mut saved = SavedNetworks::default();
        assert_eq!(saved.join_plan(), None);

        for ssid in ["far", "weak", "manual", "strong", "home"] {
            saved.connect(credentials(ssid));
        }
        saved.set_auto_connect("manual", false);

        let plan = saved.join_plan().unwrap();
        assert_eq!(plan.connected().map(WifiCredentials::ssid), Some("home"));

        let found = [
            WifiNetwork::new("weak", -80, true),
            WifiNetwork::new("home", -30, true),
            WifiNetwork::new("manual", -40, true),
            WifiNetwork::new("strong", -60, true),
            WifiNetwork::new("weak", -50, true),
        ];
        let fallbacks: Vec<_> = plan
            .fallbacks_in_range(&found)
            .into_iter()
            .map(WifiCredentials::ssid)
            .collect();

        // the connected network is tried on its own, first
        assert_eq!(fallbacks, ["weak", "strong"]);

        // with nothing connected, the auto-connect networks are still a plan
        saved.disconnect();
        let plan = saved.join_plan().unwrap();
        assert_eq!(plan.connected(), None);
        assert!(plan.has_fallbacks());
    }

    #[test]
    fn saved_networks_round_trip_with_the_device_key() {
        let mut saved = SavedNetworks::default();
        saved.connect(credentials("home"));
        saved.connect(WifiCredentials::new("cafe", "").unwrap());
        saved.set_auto_connect("home", false);
        saved.disconnect();

        let bytes = saved.encode(&KEY).unwrap();

        assert_eq!(SavedNetworks::decode(&bytes, &KEY), Ok(saved));
    }

    #[test]
    fn passwords_are_not_stored_readably() {
        let mut saved = SavedNetworks::default();
        saved.connect(WifiCredentials::new("home", "correct horse battery").unwrap());

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
                .connected()
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
        assert!(wifi.saved().connected().is_none());
    }
}
