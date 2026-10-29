use alloc::{string::String, vec::Vec};

use minicbor::{Decode, Encode};

use crate::storage::{self, StorageError};

use super::{WifiCredentials, WifiNetwork};

/// Like crosspoint, up to eight networks are remembered.
const MAX_SAVED_NETWORKS: usize = 8;

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
    pub(super) fn connect(&mut self, credentials: WifiCredentials) -> bool {
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

    pub(super) fn disconnect(&mut self) -> bool {
        core::mem::take(&mut self.connected)
    }

    /// Drops a network and its password.
    pub(super) fn forget(&mut self, ssid: &str) -> bool {
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

    pub(super) fn set_auto_connect(&mut self, ssid: &str, auto_connect: bool) -> bool {
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
}
