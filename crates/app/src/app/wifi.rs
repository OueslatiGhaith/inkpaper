use alloc::{string::String, vec::Vec};

use inkpaper_ui::prelude::*;

use super::{InkPaperApp, Screen};
use crate::{
    SavedNetworks, WifiCredentials, WifiJoinFailure, WifiNetwork, WifiScanError,
    input::LongPressEvent,
    keyboard::Key,
    screens::{wifi_networks::WifiMenuItem, wifi_password::keyboard_look},
};

impl InkPaperApp {
    pub(crate) fn apply_wifi_networks(&mut self, saved: SavedNetworks, cx: &mut Context<'_, Self>) {
        if self.wifi.apply_saved(saved) {
            cx.notify();
        }
    }

    /// Makes `credentials` the connected network and saves it.
    fn connect_wifi_network(&mut self, credentials: WifiCredentials, cx: &mut Context<'_, Self>) {
        if self.wifi.connect(credentials) {
            cx.notify();
        }
    }

    fn disconnect_wifi_network(&mut self, cx: &mut Context<'_, Self>) {
        if self.wifi.disconnect() {
            cx.notify();
        }
    }

    /// Opens the keyboard to type the password of `ssid`.
    fn enter_wifi_password(&mut self, ssid: &str, cx: &mut Context<'_, Self>) {
        self.wifi.begin_password_entry(String::from(ssid));
        self.open_screen(Screen::WifiPassword, cx);
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

    /// Tapping a network disconnects it when it's connected, and otherwise
    /// connects it: a saved or open one at once, a secured one once its
    /// password is typed.
    pub(crate) fn activate_wifi_network(
        &mut self,
        event: &ActivateEvent,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(network) = event
            .index()
            .and_then(|index| self.wifi.networks().get(index))
        else {
            return;
        };

        let ssid = String::from(network.ssid());
        self.toggle_wifi_connection(&ssid, network.secured(), cx);
    }

    fn toggle_wifi_connection(&mut self, ssid: &str, secured: bool, cx: &mut Context<'_, Self>) {
        let saved = self.wifi.saved();

        if saved
            .connected()
            .is_some_and(|connected| connected.ssid() == ssid)
        {
            self.disconnect_wifi_network(cx);
            return;
        }

        let credentials = match saved.find(ssid) {
            Some(network) => network.credentials().clone(),
            None if !secured => match WifiCredentials::new(ssid, "") {
                Some(credentials) => credentials,
                None => return,
            },
            None => {
                self.enter_wifi_password(ssid, cx);
                return;
            }
        };

        self.connect_wifi_network(credentials, cx);
    }

    /// Holding a saved network opens its menu next to the finger.
    pub(crate) fn long_press_wifi_network(
        &mut self,
        event: &LongPressEvent,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(network) = event
            .index()
            .and_then(|index| self.wifi.networks().get(index))
        else {
            return;
        };

        let ssid = String::from(network.ssid());

        if self.wifi.open_menu(&ssid, event.position().y.get()) {
            cx.notify();
        }
    }

    pub(crate) fn activate_wifi_menu_item(
        &mut self,
        event: &ActivateEvent,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(menu) = self.wifi.menu() else {
            return;
        };

        let ssid = String::from(menu.ssid());

        match event.index().and_then(WifiMenuItem::from_index) {
            Some(WifiMenuItem::Connection) => {
                self.wifi.close_menu();
                self.toggle_wifi_connection(&ssid, true, cx);
                cx.notify();
            }

            // the menu stays open to show the new setting
            Some(WifiMenuItem::AutoConnect) => {
                if self.wifi.toggle_auto_connect(&ssid) {
                    cx.notify();
                }
            }

            Some(WifiMenuItem::ChangePassword) => {
                self.wifi.close_menu();
                self.enter_wifi_password(&ssid, cx);
            }

            Some(WifiMenuItem::Forget) => {
                self.wifi.close_menu();
                self.wifi.forget(&ssid);
                cx.notify();
            }

            None => {}
        }
    }

    pub(crate) fn dismiss_wifi_menu(&mut self, _: &ActivateEvent, cx: &mut Context<'_, Self>) {
        if self.wifi.close_menu() {
            cx.notify();
        }
    }

    pub(crate) fn activate_wifi_password_key(
        &mut self,
        event: &ActivateEvent,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(key) = event.index().and_then(Key::from_code) else {
            return;
        };

        let Some(entry) = self.wifi.password_entry_mut() else {
            return;
        };

        let look = keyboard_look(entry);
        if !entry.press(key) {
            return;
        }

        // a key changes the field. The keyboard renders again only when it looks
        // different, such as after Shift
        let keyboard_changed = keyboard_look(entry) != look;
        self.refresh_password_field(cx);
        if keyboard_changed {
            self.refresh_password_keyboard(cx);
        }
    }

    /// renders the password field again, for changes nothing else shows
    pub(crate) fn refresh_password_field(&self, cx: &mut Context<'_, Self>) {
        self.password_field.update(cx, |_, cx| cx.notify()).ok();
    }

    /// renders the password keyboard again, for changes nothing else shows
    pub(crate) fn refresh_password_keyboard(&self, cx: &mut Context<'_, Self>) {
        self.password_keyboard.update(cx, |_, cx| cx.notify()).ok();
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

        self.connect_wifi_network(credentials, cx);
        self.navigate_back(cx);
    }

    pub(crate) fn activate_toggle_wifi_password(
        &mut self,
        _: &ActivateEvent,
        cx: &mut Context<'_, Self>,
    ) {
        if let Some(entry) = self.wifi.password_entry_mut() {
            entry.toggle_shown();
            self.refresh_password_field(cx);
        }
    }
}
