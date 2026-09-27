use alloc::vec;

use inkpaper_ui::prelude::*;

use crate::{
    BatteryStatus, InkPaperApp, SavedNetworks, WifiNetwork, WifiScanStatus,
    app::{Back, Entry, Exit, ScreenInput, ScreenLifecycle, ScreenView},
    components::{
        header::{BackHeader, BackHeaderProps},
        popup_menu::{MenuItem, PopupMenu, PopupMenuProps},
        settings_row::{ListRow, ListRowProps, SettingsValueRow, SettingsValueRowProps},
    },
    input::LongPressEvent,
    wifi::NetworkMenu,
};

/// The items of a saved network's menu, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WifiMenuItem {
    /// Connect or Disconnect
    Connection,
    AutoConnect,
    ChangePassword,
    Forget,
}

impl WifiMenuItem {
    const ALL: [Self; 4] = [
        Self::Connection,
        Self::AutoConnect,
        Self::ChangePassword,
        Self::Forget,
    ];

    pub(crate) fn from_index(index: usize) -> Option<Self> {
        Self::ALL.get(index).copied()
    }
}

/// Networks around the device, like crosspoint's WiFi selection. Tapping a
/// network connects or disconnects it; holding a saved one opens its menu.
#[component]
pub(crate) struct WifiNetworksScreen<'a> {
    networks: &'a [WifiNetwork],
    saved: &'a SavedNetworks,
    menu: Option<&'a NetworkMenu>,
    on_network: Listener<ActivateEvent>,
    on_hold_network: Listener<LongPressEvent>,
    on_menu_item: Listener<ActivateEvent>,
    on_dismiss_menu: Listener<ActivateEvent>,
    scan: WifiScanStatus,
    revision: u64,
    battery: Option<BatteryStatus>,
    on_back: Listener<ActivateEvent>,
    on_scan: Listener<ActivateEvent>,
}

impl RenderOnce for WifiNetworksScreen<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let status = match self.scan {
            WifiScanStatus::Idle | WifiScanStatus::Scanning => "Scanning...",
            WifiScanStatus::Failed => "Could not scan for networks",
            WifiScanStatus::Done if self.networks.is_empty() => "No networks found",
            WifiScanStatus::Done => "Tap to connect, hold for options",
        };

        let menu = self.menu.map(|menu| {
            let connected = self
                .saved
                .connected()
                .is_some_and(|network| network.ssid() == menu.ssid());
            let auto_connect = self
                .saved
                .find(menu.ssid())
                .is_some_and(|network| network.auto_connect());

            PopupMenu::from(PopupMenuProps {
                anchor_y: menu.top(),
                items: vec![
                    MenuItem {
                        label: if connected { "Disconnect" } else { "Connect" },
                        value: "",
                    },
                    MenuItem {
                        label: "Auto-connect",
                        value: if auto_connect { "On" } else { "Off" },
                    },
                    MenuItem {
                        label: "Change Password",
                        value: "",
                    },
                    MenuItem {
                        label: "Forget",
                        value: "",
                    },
                ],
                on_item: self.on_menu_item,
                on_dismiss: self.on_dismiss_menu,
            })
        });

        // a running scan can't be started again
        let on_scan = (self.scan != WifiScanStatus::Scanning).then_some(self.on_scan);

        rsx! {
            <div class="w-[480px] h-[800px] relative bg-white text-black">
                <div class="absolute left-0 top-[5px] w-[480px] h-[77px]">
                    <BackHeader
                        title="WiFi"
                        battery={self.battery}
                        on_back={self.on_back}
                    />
                </div>

                <div class="absolute left-7 top-[98px] w-[424px] h-8 flex items-center">
                    <text class="text-base no-wrap max-lines-1 text-ellipsis">
                        {status}
                    </text>
                </div>

                <div class="absolute left-0 top-[130px] w-[480px]">
                    <ListRow
                        id={("wifi-scan", 0)}
                        label="Scan Again"
                        depth={0}
                        selected={false}
                        chevron={false}
                        on_activate={on_scan}
                    />
                </div>

                <div class="absolute left-0 top-[202px] w-[480px] h-[598px]">
                    <NetworkList
                        networks={self.networks}
                        saved={self.saved}
                        on_network={self.on_network}
                        on_hold_network={self.on_hold_network}
                        revision={self.revision}
                    />
                </div>

                {#if let Some(menu) = menu}
                    {menu}
                {/if}
            </div>
        }
    }
}

#[component]
struct NetworkList<'a> {
    networks: &'a [WifiNetwork],
    saved: &'a SavedNetworks,
    on_network: Listener<ActivateEvent>,
    on_hold_network: Listener<LongPressEvent>,
    revision: u64,
}

impl RenderOnce for NetworkList<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let saved = self.saved;
        let connected = saved.connected().map(|network| network.ssid());
        let (on_network, on_hold_network) = (self.on_network, self.on_hold_network);

        // every row shares one listener per event; the row's id says which
        let rows = self
            .networks
            .iter()
            .enumerate()
            .map(move |(index, network)| {
                let known = saved.find(network.ssid()).is_some();

                let value = if connected == Some(network.ssid()) {
                    "Connected"
                } else if known {
                    "Saved"
                } else if network.secured() {
                    "Password"
                } else {
                    "Open"
                };

                let row = SettingsValueRow::from(SettingsValueRowProps {
                    id: ("wifi-network", index),
                    label: network.ssid(),
                    value,
                    selected: false,
                    on_activate: Some(on_network),
                });

                // only saved networks have a menu
                div()
                    .id(("wifi-network-hold", index))
                    .when(known, |hold| hold.on::<LongPressEvent>(on_hold_network))
                    .w_full()
                    .child(row)
            });

        div()
            .id(("wifi-network-list", self.revision))
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .overflow_y_scroll()
            .children(rows)
    }
}

pub(crate) struct WifiNetworksRoute;

impl ScreenLifecycle for WifiNetworksRoute {
    fn enter(&self, app: &mut InkPaperApp, entry: Entry) {
        if entry == Entry::Opened {
            app.wifi.request_scan();
        }
    }

    fn exit(&self, app: &mut InkPaperApp, _exit: Exit) {
        app.wifi.close_menu();
    }

    // Back closes an open menu first
    fn back(&self, app: &mut InkPaperApp, cx: &mut Context<'_, InkPaperApp>) -> Back {
        if !app.wifi.close_menu() {
            return Back::Leave;
        }

        cx.notify();

        Back::Handled
    }
}

// while the menu is open, the list behind it neither scrolls nor opens
// another menu
impl ScreenInput for WifiNetworksRoute {
    fn drag(
        &self,
        app: &mut InkPaperApp,
        _origin: Point,
        _position: Point,
        _cx: &mut Context<'_, InkPaperApp>,
    ) -> bool {
        app.wifi.menu().is_some()
    }

    fn long_press(
        &self,
        app: &mut InkPaperApp,
        _position: Point,
        _cx: &mut Context<'_, InkPaperApp>,
    ) -> bool {
        app.wifi.menu().is_some()
    }
}

impl ScreenView for WifiNetworksRoute {
    fn render<'a>(
        &self,
        app: &'a InkPaperApp,
        cx: &mut Context<'_, InkPaperApp>,
    ) -> AnyElement<'a> {
        WifiNetworksScreen::from(WifiNetworksScreenProps {
            networks: app.wifi.networks(),
            saved: app.wifi.saved(),
            menu: app.wifi.menu(),
            on_network: cx.listener(InkPaperApp::activate_wifi_network),
            on_hold_network: cx.listener(InkPaperApp::long_press_wifi_network),
            on_menu_item: cx.listener(InkPaperApp::activate_wifi_menu_item),
            on_dismiss_menu: cx.listener(InkPaperApp::dismiss_wifi_menu),
            scan: app.wifi.scan_status(),
            revision: app.wifi.revision(),
            battery: app.system_status.battery(),
            on_back: cx.listener(InkPaperApp::activate_back),
            on_scan: cx.listener(InkPaperApp::activate_wifi_scan),
        })
        .into_any_element()
    }
}
