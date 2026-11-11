use alloc::{string::String, vec, vec::Vec};

use inkpaper_ui::prelude::*;

use crate::{
    InkPaperApp, WifiScanStatus,
    app::{Back, Entry, Exit, ScreenInput, ScreenLifecycle, ScreenView, SideButton},
    components::{
        header::{BackHeader, BackHeaderProps, BatteryIndicator},
        page_number::{PageNumber, PageNumberProps},
        popup_menu::{MenuItem, PopupMenu, PopupMenuProps},
        settings_row::{
            LIST_ROW_HEIGHT, ListRow, ListRowProps, SettingsValueRow, SettingsValueRowProps,
        },
    },
    input::LongPressEvent,
    paging::{self, PageTurn, Pager},
};

/// The list fills the space under Scan Again.
const LIST_HEIGHT: i32 = 598;
const ROWS_PER_PAGE: usize = (LIST_HEIGHT / LIST_ROW_HEIGHT) as usize;

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

/// A network row on the page shown.
#[derive(Debug, Clone, PartialEq, Eq)]
struct NetworkRow {
    /// its index among the networks found
    index: usize,
    ssid: String,
    /// Connected, Saved, Password or Open
    value: &'static str,
    /// saved networks have a menu
    known: bool,
}

/// The long-press menu of a saved network, `top` px down the screen.
#[derive(Debug, Clone, PartialEq, Eq)]
struct NetworkMenu {
    ssid: String,
    top: i32,
}

/// What the menu shows of its network.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MenuState {
    top: i32,
    connected: bool,
    auto_connect: bool,
}

/// Networks around the device, like crosspoint's WiFi selection. Tapping a
/// network connects or disconnects it; holding a saved one opens its menu.
#[component]
pub(crate) struct WifiNetworksScreen {
    /// the networks on the page shown
    rows: Vec<NetworkRow>,
    /// whether the scan found none
    empty: bool,
    menu: Option<MenuState>,
    on_network: Listener<ActivateEvent>,
    on_hold_network: Listener<LongPressEvent>,
    on_menu_item: Listener<ActivateEvent>,
    on_dismiss_menu: Listener<ActivateEvent>,
    scan: WifiScanStatus,
    revision: u64,
    page: usize,
    page_count: usize,
    battery: Entity<BatteryIndicator>,
    on_back: Listener<ActivateEvent>,
    on_scan: Listener<ActivateEvent>,
}

impl RenderOnce for WifiNetworksScreen {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let status = match self.scan {
            WifiScanStatus::Idle | WifiScanStatus::Scanning => "Scanning...",
            WifiScanStatus::Failed => "Could not scan for networks",
            WifiScanStatus::Done if self.empty => "No networks found",
            WifiScanStatus::Done => "Tap to connect, hold for options",
        };

        let menu = self.menu.map(|menu| {
            PopupMenu::from(PopupMenuProps {
                anchor_y: menu.top,
                items: vec![
                    MenuItem {
                        label: if menu.connected {
                            "Disconnect"
                        } else {
                            "Connect"
                        },
                        value: "",
                    },
                    MenuItem {
                        label: "Auto-connect",
                        value: if menu.auto_connect { "On" } else { "Off" },
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

        let list_height = px(LIST_HEIGHT);
        let paged = self.page_count > 1;
        let status_width = if paged { px(344) } else { px(424) };

        rsx! {
            <div class="w-[480px] h-[800px] relative bg-white text-black">
                <div class="absolute left-0 top-[5px] w-[480px] h-[77px]">
                    <BackHeader
                        title="WiFi"
                        battery={self.battery}
                        on_back={self.on_back}
                    />
                </div>

                <div class="absolute left-7 top-[98px] w-{status_width} h-8 flex items-center">
                    <text class="text-base no-wrap max-lines-1 text-ellipsis">
                        {status}
                    </text>
                </div>

                {#if paged}
                    <div class="absolute right-7 top-[98px] h-8 flex items-center">
                        <PageNumber page={self.page} page_count={self.page_count} />
                    </div>
                {/if}

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

                <div class="absolute left-0 top-[202px] w-[480px] h-{list_height}">
                    <NetworkList
                        rows={self.rows}
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

/// One page of the networks.
#[component]
struct NetworkList {
    rows: Vec<NetworkRow>,
    on_network: Listener<ActivateEvent>,
    on_hold_network: Listener<LongPressEvent>,
    revision: u64,
}

impl RenderOnce for NetworkList {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let (on_network, on_hold_network) = (self.on_network, self.on_hold_network);

        // every row shares one listener per event; the row's id says which
        let rows = self.rows.into_iter().map(move |row| {
            let item = SettingsValueRow::from(SettingsValueRowProps {
                id: ("wifi-network", row.index),
                label: row.ssid.into(),
                value: row.value.into(),
                selected: false,
                on_activate: Some(on_network),
            });

            // only saved networks have a menu
            div()
                .id(("wifi-network-hold", row.index))
                .when(row.known, |hold| hold.on::<LongPressEvent>(on_hold_network))
                .w_full()
                .child(item)
        });

        div()
            .id(("wifi-network-list", self.revision))
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .children(rows)
    }
}

/// What the WiFi screen asks of the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WifiNetworksEvent {
    Back,
    Scan,
    /// network `0` of the networks found was tapped
    Network(usize),
    /// an item of the open menu was tapped. The menu stays open until the app
    /// closes it, so it can tell which network the menu is for
    MenuItem(WifiMenuItem),
}

/// the WiFi screen. As its own entity it keeps its page and the menu held open. The
/// networks found and saved are the app's, which handles taps as
/// [`WifiNetworksEvent`]s.
pub(crate) struct WifiNetworksView {
    app: Entity<InkPaperApp>,
    pager: Pager,
    menu: Option<NetworkMenu>,
    /// how many networks there were at the last render
    network_count: usize,
}

impl EventEmitter<WifiNetworksEvent> for WifiNetworksView {}

impl WifiNetworksView {
    pub(crate) const fn new(app: Entity<InkPaperApp>) -> Self {
        Self {
            app,
            pager: Pager::showing(0),
            menu: None,
            network_count: 0,
        }
    }

    /// Shows the first page, as after a scan.
    pub(crate) fn reset_page(&mut self, cx: &mut Context<'_, Self>) {
        if self.pager != Pager::default() {
            self.pager = Pager::default();
            cx.notify();
        }
    }

    pub(crate) const fn menu_open(&self) -> bool {
        self.menu.is_some()
    }

    /// The network the open menu is for.
    pub(crate) fn menu_ssid(&self) -> Option<&str> {
        self.menu.as_ref().map(|menu| menu.ssid.as_str())
    }

    /// Returns whether the menu was open.
    pub(crate) fn close_menu(&mut self, cx: &mut Context<'_, Self>) -> bool {
        let closed = self.menu.take().is_some();

        if closed {
            cx.notify();
        }

        closed
    }

    /// While the menu is open, the list behind it doesn't turn pages.
    pub(crate) fn turn_page(&mut self, turn: PageTurn, cx: &mut Context<'_, Self>) {
        if self.menu.is_none() && self.pager.turn(turn, self.network_count, ROWS_PER_PAGE) {
            cx.notify();
        }
    }

    fn activate_back(&mut self, _: &ActivateEvent, cx: &mut Context<'_, Self>) {
        cx.emit(WifiNetworksEvent::Back);
    }

    fn activate_scan(&mut self, _: &ActivateEvent, cx: &mut Context<'_, Self>) {
        cx.emit(WifiNetworksEvent::Scan);
    }

    fn activate_network(&mut self, event: &ActivateEvent, cx: &mut Context<'_, Self>) {
        if let Some(index) = event.index() {
            cx.emit(WifiNetworksEvent::Network(index));
        }
    }

    /// Holding a saved network opens its menu next to the finger.
    fn long_press_network(&mut self, event: &LongPressEvent, cx: &mut Context<'_, Self>) {
        let Some(index) = event.index() else {
            return;
        };

        let ssid = self.app.read(cx, |app| {
            let network = app.wifi.networks().get(index)?;

            app.wifi
                .saved()
                .find(network.ssid())
                .map(|_| String::from(network.ssid()))
        });

        if let Some(ssid) = ssid {
            self.menu = Some(NetworkMenu {
                ssid,
                top: event.position().y.get(),
            });
            cx.notify();
        }
    }

    fn activate_menu_item(&mut self, event: &ActivateEvent, cx: &mut Context<'_, Self>) {
        if let Some(item) = event.index().and_then(WifiMenuItem::from_index) {
            cx.emit(WifiNetworksEvent::MenuItem(item));
        }
    }

    fn activate_dismiss_menu(&mut self, _: &ActivateEvent, cx: &mut Context<'_, Self>) {
        self.close_menu(cx);
    }
}

impl Render for WifiNetworksView {
    fn render(&mut self, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let pager = self.pager;
        let menu = self.menu.as_ref();

        let (rows, network_count, menu, scan, revision, battery) = self.app.read(cx, |app| {
            let networks = app.wifi.networks();
            let saved = app.wifi.saved();
            let connected = saved.connected().map(|network| network.ssid());
            let page = pager.page(networks.len(), ROWS_PER_PAGE);

            let rows = networks
                .iter()
                .enumerate()
                .skip(page * ROWS_PER_PAGE)
                .take(ROWS_PER_PAGE)
                .map(|(index, network)| {
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

                    NetworkRow {
                        index,
                        ssid: String::from(network.ssid()),
                        value,
                        known,
                    }
                })
                .collect::<Vec<_>>();

            let menu = menu.map(|menu| MenuState {
                top: menu.top,
                connected: connected == Some(menu.ssid.as_str()),
                auto_connect: saved
                    .find(&menu.ssid)
                    .is_some_and(|network| network.auto_connect()),
            });

            (
                rows,
                networks.len(),
                menu,
                app.wifi.scan_status(),
                app.wifi.revision(),
                app.battery_indicator,
            )
        });

        self.network_count = network_count;

        WifiNetworksScreen::from(WifiNetworksScreenProps {
            rows,
            empty: network_count == 0,
            menu,
            on_network: cx.listener(Self::activate_network),
            on_hold_network: cx.listener(Self::long_press_network),
            on_menu_item: cx.listener(Self::activate_menu_item),
            on_dismiss_menu: cx.listener(Self::activate_dismiss_menu),
            scan,
            revision,
            page: pager.page(network_count, ROWS_PER_PAGE),
            page_count: Pager::page_count(network_count, ROWS_PER_PAGE),
            battery,
            on_back: cx.listener(Self::activate_back),
            on_scan: cx.listener(Self::activate_scan),
        })
    }
}

pub(crate) struct WifiNetworksRoute;

impl ScreenLifecycle for WifiNetworksRoute {
    fn enter(&self, app: &mut InkPaperApp, entry: Entry, _cx: &mut Context<'_, InkPaperApp>) {
        if entry == Entry::Opened {
            app.wifi.request_scan();
        }
    }

    fn exit(&self, app: &mut InkPaperApp, _exit: Exit, cx: &mut Context<'_, InkPaperApp>) {
        app.wifi_networks_view
            .update(cx, WifiNetworksView::close_menu);
    }

    // Back closes an open menu first
    fn back(&self, app: &mut InkPaperApp, cx: &mut Context<'_, InkPaperApp>) -> Back {
        if app
            .wifi_networks_view
            .update(cx, WifiNetworksView::close_menu)
        {
            return Back::Handled;
        }

        Back::Leave
    }
}

// while the menu is open, the list behind it neither turns pages nor opens
// another menu
impl ScreenInput for WifiNetworksRoute {
    fn side_button(
        &self,
        app: &mut InkPaperApp,
        button: SideButton,
        cx: &mut Context<'_, InkPaperApp>,
    ) {
        app.wifi_networks_view
            .update(cx, |view, cx| view.turn_page(button.into(), cx));
    }

    fn drag(
        &self,
        app: &mut InkPaperApp,
        origin: Point,
        position: Point,
        cx: &mut Context<'_, InkPaperApp>,
    ) {
        let view = app.wifi_networks_view;

        if view.read(cx, WifiNetworksView::menu_open) {
            return;
        }

        // once a swipe turns a page, the rest of the touch turns nothing more
        if let Some(turn) = paging::swipe_turn(origin, position) {
            view.update(cx, |view, cx| view.turn_page(turn, cx));
            app.capture_pointer();
        }
    }

    fn long_press(
        &self,
        app: &mut InkPaperApp,
        _position: Point,
        cx: &mut Context<'_, InkPaperApp>,
    ) -> bool {
        app.wifi_networks_view.read(cx, WifiNetworksView::menu_open)
    }
}

impl ScreenView for WifiNetworksRoute {
    fn render<'a>(&self, app: &'a InkPaperApp, _: &mut Context<'_, InkPaperApp>) -> AnyElement<'a> {
        app.wifi_networks_view.into_any_element()
    }
}
