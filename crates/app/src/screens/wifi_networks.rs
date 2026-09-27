use inkpaper_ui::prelude::*;

use crate::{
    BatteryStatus, InkPaperApp, SavedNetworks, WifiNetwork, WifiScanStatus,
    app::{Entry, ScreenInput, ScreenLifecycle, ScreenView},
    components::{
        header::{BackHeader, BackHeaderProps},
        settings_row::{ListRow, ListRowProps, SettingsValueRow, SettingsValueRowProps},
    },
};

/// Networks around the device, like crosspoint's WiFi selection. Saved and
/// open networks can be chosen; the first saved one is the one in use.
#[component]
pub(crate) struct WifiNetworksScreen<'a> {
    networks: &'a [WifiNetwork],
    saved: &'a SavedNetworks,
    on_network: Listener<ActivateEvent>,
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
            WifiScanStatus::Done => "Choose the network to use",
        };

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
                        revision={self.revision}
                    />
                </div>
            </div>
        }
    }
}

#[component]
struct NetworkList<'a> {
    networks: &'a [WifiNetwork],
    saved: &'a SavedNetworks,
    on_network: Listener<ActivateEvent>,
    revision: u64,
}

impl RenderOnce for NetworkList<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let saved = self.saved;
        let current = saved.current().map(|network| network.ssid());
        let on_network = self.on_network;

        // every row shares one listener; the row's id says which was tapped
        let rows = self
            .networks
            .iter()
            .enumerate()
            .map(move |(index, network)| {
                let known = saved.find(network.ssid()).is_some();

                let value = if current == Some(network.ssid()) {
                    "In use"
                } else if known {
                    "Saved"
                } else if network.secured() {
                    "Password"
                } else {
                    "Open"
                };

                SettingsValueRow::from(SettingsValueRowProps {
                    id: ("wifi-network", index),
                    label: network.ssid(),
                    value,
                    selected: false,
                    on_activate: Some(on_network),
                })
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
}

impl ScreenInput for WifiNetworksRoute {}

impl ScreenView for WifiNetworksRoute {
    fn render<'a>(
        &self,
        app: &'a InkPaperApp,
        cx: &mut Context<'_, InkPaperApp>,
    ) -> AnyElement<'a> {
        WifiNetworksScreen::from(WifiNetworksScreenProps {
            networks: app.wifi.networks(),
            saved: app.wifi.saved(),
            on_network: cx.listener(InkPaperApp::activate_wifi_network),
            scan: app.wifi.scan_status(),
            revision: app.wifi.revision(),
            battery: app.system_status.battery(),
            on_back: cx.listener(InkPaperApp::activate_back),
            on_scan: cx.listener(InkPaperApp::activate_wifi_scan),
        })
        .into_any_element()
    }
}
