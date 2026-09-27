use alloc::{string::String, vec::Vec};

use inkpaper_ui::prelude::*;

use super::{InkPaperApp, Screen};
use crate::{
    SavedNetworks, WifiCredentials, WifiJoinFailure, WifiNetwork, WifiScanError, keyboard::Key,
};

impl InkPaperApp {
    pub(crate) fn apply_wifi_networks(&mut self, saved: SavedNetworks, cx: &mut Context<'_, Self>) {
        if self.wifi.apply_saved(saved) {
            cx.notify();
        }
    }

    /// Makes `credentials` the network the device uses and saves it.
    fn remember_wifi_network(&mut self, credentials: WifiCredentials, cx: &mut Context<'_, Self>) {
        if self.wifi.remember(credentials) {
            cx.notify();
        }
    }

    pub(crate) fn take_wifi_networks_save_request(&mut self) -> Option<SavedNetworks> {
        self.wifi.take_save_request()
    }

    pub(crate) fn activate_wifi_scan(&mut self, _: &ActivateEvent, cx: &mut Context<'_, Self>) {
        if self.wifi.request_scan() {
            cx.notify();
        }
    }

    pub(crate) fn take_wifi_scan_request(&mut self) -> bool {
        self.wifi.take_scan_request()
    }

    /// Reports the networks a scan started by the app found.
    pub fn apply_wifi_scan_result(
        &mut self,
        result: Result<Vec<WifiNetwork>, WifiScanError>,
        cx: &mut Context<'_, Self>,
    ) {
        self.wifi.finish_scan(result);

        if self.screen() == Screen::WifiNetworks {
            cx.notify();
        }
    }

    /// Chooses a scanned network: a saved one with its password, an open one, or
    /// a secured one after its password is typed.
    pub(crate) fn activate_wifi_network(
        &mut self,
        event: &ActivateEvent,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(index) = event.index() else {
            return;
        };

        let Some(network) = self.wifi.networks().get(index) else {
            return;
        };

        let credentials = match self.wifi.saved().find(network.ssid()) {
            Some(saved) => saved.clone(),
            None if !network.secured() => match WifiCredentials::new(network.ssid(), "") {
                Some(credentials) => credentials,
                None => return,
            },
            None => {
                self.wifi.begin_password_entry(String::from(network.ssid()));
                self.open_screen(Screen::WifiPassword, cx);

                return;
            }
        };

        self.remember_wifi_network(credentials, cx);
    }

    pub(crate) fn activate_wifi_password_key(
        &mut self,
        event: &ActivateEvent,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(key) = event.index().and_then(Key::from_code) else {
            return;
        };

        if self
            .wifi
            .password_entry_mut()
            .is_some_and(|entry| entry.press(key))
        {
            cx.notify();
        }
    }

    pub(crate) fn take_wifi_join_request(&mut self) -> Option<WifiCredentials> {
        self.wifi.take_join_request()
    }

    /// Reports how joining a network with a typed password ended. A network
    /// that joined is saved as the one in use.
    pub fn apply_wifi_join_result(
        &mut self,
        result: Result<(), WifiJoinFailure>,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(credentials) = self.wifi.finish_join(result) else {
            if self.screen() == Screen::WifiPassword {
                cx.notify();
            }

            return;
        };

        self.remember_wifi_network(credentials, cx);
        self.navigate_back(cx);
    }

    pub(crate) fn activate_toggle_wifi_password(
        &mut self,
        _: &ActivateEvent,
        cx: &mut Context<'_, Self>,
    ) {
        if let Some(entry) = self.wifi.password_entry_mut() {
            entry.toggle_shown();
            cx.notify();
        }
    }
}
