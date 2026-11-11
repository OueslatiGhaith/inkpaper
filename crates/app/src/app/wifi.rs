use alloc::{string::String, vec::Vec};

use inkpaper_ui::prelude::*;

use super::{InkPaperApp, Screen};
use crate::{
    SavedNetworks, WifiCredentials, WifiJoinFailure, WifiNetwork, WifiScanError,
    screens::{
        wifi_networks::{WifiMenuItem, WifiNetworksEvent, WifiNetworksView},
        wifi_password::{
            PasswordFieldView, PasswordKeyboardView, PressKey, TogglePassword, keyboard_look,
        },
    },
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

    /// What the WiFi screen's taps mean.
    pub(super) fn on_wifi_networks_event(
        &mut self,
        view: Entity<WifiNetworksView>,
        event: &WifiNetworksEvent,
        cx: &mut Context<'_, Self>,
    ) {
        match *event {
            WifiNetworksEvent::Back => self.navigate_back(cx),
            WifiNetworksEvent::Scan => {
                if self.wifi.request_scan() {
                    cx.notify();
                }
            }
            WifiNetworksEvent::Network(index) => self.activate_wifi_network(index, cx),
            WifiNetworksEvent::MenuItem(item) => self.activate_wifi_menu_item(view, item, cx),
        }
    }

    /// Reports the networks a scan started by the app found.
    pub fn apply_wifi_scan_result(
        &mut self,
        result: Result<Vec<WifiNetwork>, WifiScanError>,
        cx: &mut Context<'_, Self>,
    ) {
        self.wifi.finish_scan(result);
        self.wifi_networks_view
            .update(cx, WifiNetworksView::reset_page);

        if self.screen() == Screen::WifiNetworks {
            cx.notify();
        }
    }

    /// Tapping a network disconnects it when it's connected, and otherwise
    /// connects it: a saved or open one at once, a secured one once its
    /// password is typed.
    fn activate_wifi_network(&mut self, index: usize, cx: &mut Context<'_, Self>) {
        let Some(network) = self.wifi.networks().get(index) else {
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

    /// An item of the menu of the network `view` holds open.
    fn activate_wifi_menu_item(
        &mut self,
        view: Entity<WifiNetworksView>,
        item: WifiMenuItem,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(ssid) = view.read_with(cx, |view| view.menu_ssid().map(String::from)) else {
            return;
        };

        // the menu stays open to show a new Auto-connect setting
        if item != WifiMenuItem::AutoConnect {
            view.update(cx, WifiNetworksView::close_menu);
        }

        match item {
            WifiMenuItem::Connection => {
                self.toggle_wifi_connection(&ssid, true, cx);
                cx.notify();
            }

            WifiMenuItem::AutoConnect => {
                if self.wifi.toggle_auto_connect(&ssid) {
                    cx.notify();
                }
            }

            WifiMenuItem::ChangePassword => self.enter_wifi_password(&ssid, cx),

            WifiMenuItem::Forget => {
                self.wifi.forget(&ssid);
                cx.notify();
            }
        }
    }

    /// Shows or hides the password typed.
    pub(super) fn on_toggle_password(
        &mut self,
        field: Entity<PasswordFieldView>,
        _: &TogglePassword,
        cx: &mut Context<'_, Self>,
    ) {
        if let Some(entry) = self.wifi.password_entry_mut() {
            entry.toggle_shown();
            field.notify(cx);
        }
    }

    /// Types a key of the password keyboard.
    pub(super) fn on_password_key(
        &mut self,
        keyboard: Entity<PasswordKeyboardView>,
        &PressKey(key): &PressKey,
        cx: &mut Context<'_, Self>,
    ) {
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
        self.password_field.notify(cx);
        if keyboard_changed {
            keyboard.notify(cx);
        }
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
}
