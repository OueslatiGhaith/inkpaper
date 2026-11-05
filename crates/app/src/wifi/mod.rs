mod password;
mod saved;

use alloc::{string::String, vec::Vec};

pub(crate) use password::{JoinStatus, PasswordEntry};
use saved::SavedNetwork;
pub(crate) use saved::SavedNetworks;
pub use saved::{DeviceKey, WifiJoinPlan};

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

    /// Changes whenever the scan list is replaced.
    pub(crate) const fn revision(&self) -> u64 {
        self.revision
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
    use crate::keyboard::Key;

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
