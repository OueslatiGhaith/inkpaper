use inkpaper_ui::{FontRegistryError, prelude::*};

use crate::{
    ClockState, CoverState, FileTransferState, FrontlightState,
    browser::BrowserState,
    components::header::BatteryIndicator,
    components::{
        control_center::{ControlCenter, ControlCenterProps, FrontlightPanel},
        reader_menu::ReaderDrawer,
        toast::{Toast, ToastId, ToastLayer},
    },
    control_center::ControlCenterState,
    dictionaries::{DictionarySettings, WordLookup},
    reader::{ReaderState, TextSettingsState},
    reading_history::ReadingHistoryState,
    screens::{
        clock_settings::TimezoneView,
        settings::SettingsView,
        text_settings::TextSettingsView,
        wifi_networks::WifiNetworksView,
        wifi_password::{PasswordFieldView, PasswordKeyboardView},
    },
    screens::{
        reader::ReaderBatteryIcon,
        sleep::{SleepScreen, SleepScreenProps},
    },
    system::{BatteryState, RtcState},
    wifi::WifiState,
};

mod browse;
mod dictionaries;
mod file_transfer;
mod history;
mod input;
mod navigation;
mod reader;
mod request;
mod settings;
mod system;
mod text_settings;
mod wifi;

pub(crate) use input::{ScreenInput, SideButton};
use navigation::NavigationStack;
pub(crate) use navigation::{Back, Entry, Exit, ScreenLifecycle};
pub(crate) use request::AppRequest;

use crate::screens::{
    browse_files::BrowseFilesRoute, clock_settings::ClockSettingsRoute,
    file_transfer::FileTransferRoute, home::HomeRoute, reader::ReaderRoute,
    recent_books::RecentBooksRoute, settings::SettingsRoute,
    table_of_contents::TableOfContentsRoute, text_settings::TextSettingsRoute,
    wifi_networks::WifiNetworksRoute, wifi_password::WifiPasswordRoute,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Screen {
    Home,
    BrowseFiles,
    Reader,
    RecentBooks,
    FileTransfer,
    Settings,
    ClockSettings,
    WifiNetworks,
    WifiPassword,
    TableOfContents,
    TextSettings,
}

/// How a screen draws itself.
pub(crate) trait ScreenView {
    fn render<'a>(&self, app: &'a InkPaperApp, cx: &mut Context<'_, InkPaperApp>)
    -> AnyElement<'a>;
}

/// Everything a screen defines about its own behavior.
trait ScreenRoute: ScreenLifecycle + ScreenInput + ScreenView {}

impl<T: ScreenLifecycle + ScreenInput + ScreenView> ScreenRoute for T {}

impl Screen {
    fn route(self) -> &'static dyn ScreenRoute {
        match self {
            Self::Home => &HomeRoute,
            Self::BrowseFiles => &BrowseFilesRoute,
            Self::Reader => &ReaderRoute,
            Self::RecentBooks => &RecentBooksRoute,
            Self::FileTransfer => &FileTransferRoute,
            Self::Settings => &SettingsRoute,
            Self::ClockSettings => &ClockSettingsRoute,
            Self::WifiNetworks => &WifiNetworksRoute,
            Self::WifiPassword => &WifiPasswordRoute,
            Self::TableOfContents => &TableOfContentsRoute,
            Self::TextSettings => &TextSettingsRoute,
        }
    }
}

pub struct InkPaperApp {
    navigation: NavigationStack,
    sleeping: bool,
    pub(crate) browser: BrowserState,
    pub(crate) reader: ReaderState,
    /// the Text Settings screen
    pub(crate) text_settings: TextSettingsState,
    /// the dictionaries Settings offers
    pub(crate) dictionaries: DictionarySettings,
    /// the word looked up in the dictionary, and its definition
    pub(crate) lookup: WordLookup,
    pub(crate) reading_history: ReadingHistoryState,
    /// the current book's cover on the home screen
    pub(crate) cover: CoverState,
    pub(crate) battery: Entity<BatteryState>,
    pub(crate) rtc: Entity<RtcState>,
    /// the battery in screen headers
    pub(crate) battery_indicator: Entity<BatteryIndicator>,
    /// the battery in the reader's status bar
    pub(crate) reader_battery_icon: Entity<ReaderBatteryIcon>,
    pub(crate) clock: ClockState,
    pub(crate) wifi: WifiState,
    frontlight: Entity<FrontlightState>,
    /// the frontlight controls of the control center
    frontlight_panel: Entity<FrontlightPanel>,
    /// the reader's drawer
    pub(crate) reader_menu: Entity<ReaderDrawer>,
    /// the Wi-Fi password field and its keyboard
    pub(crate) password_field: Entity<PasswordFieldView>,
    pub(crate) password_keyboard: Entity<PasswordKeyboardView>,
    /// the local time and timezone slider of the clock settings
    pub(crate) timezone: Entity<TimezoneView>,
    pub(crate) settings: Entity<SettingsView>,
    pub(crate) text_settings_view: Entity<TextSettingsView>,
    pub(crate) wifi_networks_view: Entity<WifiNetworksView>,
    control_center: ControlCenterState,
    /// the toast over every screen
    toasts: Entity<ToastLayer>,
    /// set once a long press took the current touch
    pointer_captured: bool,
    /// set once the screen's own long press took the current touch, so its
    /// moves and release still go to the screen
    pointer_held: bool,
    pub(crate) file_transfer: FileTransferState,
}

impl InkPaperApp {
    pub fn new(cx: &mut Context<'_, Self>) -> Self {
        let battery = cx.new(|_| BatteryState::default());
        let rtc = cx.new(|_| RtcState::default());
        let battery_indicator = cx.new(|_| BatteryIndicator::new(battery));
        let reader_battery_icon = cx.new(|_| ReaderBatteryIcon::default());
        let frontlight = cx.new(|_| FrontlightState::default());
        let frontlight_panel = cx.new(|_| FrontlightPanel::new(frontlight));
        let app = cx.entity();
        let reader_menu = cx.new(|_| ReaderDrawer::new(app));
        cx.subscribe(reader_menu, Self::on_reader_drawer_event);
        let password_field = cx.new(|_| PasswordFieldView::new(app));
        cx.subscribe(password_field, Self::on_toggle_password);
        let password_keyboard = cx.new(|_| PasswordKeyboardView::new(app));
        cx.subscribe(password_keyboard, Self::on_password_key);
        let timezone = cx.new(|_| TimezoneView::new(app));
        cx.subscribe(timezone, Self::on_step_timezone);
        let settings = cx.new(|_| SettingsView::new(app));
        cx.subscribe(settings, Self::on_settings_event);
        let text_settings_view = cx.new(|_| TextSettingsView::new(app));
        cx.subscribe(text_settings_view, Self::on_text_settings_event);
        let wifi_networks_view = cx.new(|_| WifiNetworksView::new(app));
        cx.subscribe(wifi_networks_view, Self::on_wifi_networks_event);
        let toasts = cx.new(|_| ToastLayer::default());

        Self {
            navigation: NavigationStack::default(),
            sleeping: false,
            browser: BrowserState::default(),
            reader: ReaderState::default(),
            text_settings: TextSettingsState::default(),
            dictionaries: DictionarySettings::default(),
            lookup: WordLookup::default(),
            reading_history: ReadingHistoryState::default(),
            cover: CoverState::default(),
            battery,
            rtc,
            battery_indicator,
            reader_battery_icon,
            clock: ClockState::default(),
            wifi: WifiState::default(),
            frontlight,
            frontlight_panel,
            reader_menu,
            password_field,
            password_keyboard,
            timezone,
            settings,
            text_settings_view,
            wifi_networks_view,
            control_center: ControlCenterState::default(),
            toasts,
            pointer_captured: false,
            pointer_held: false,
            file_transfer: FileTransferState::default(),
        }
    }

    /// Shows `toast` over the screen, in place of any shown.
    pub(crate) fn show_toast(&mut self, toast: Toast, cx: &mut Context<'_, Self>) {
        self.toasts.update(cx, |toasts, cx| toasts.show(toast, cx));
    }

    /// Dismisses toast `id`, if it's still shown.
    pub(crate) fn dismiss_toast(&mut self, id: ToastId, cx: &mut Context<'_, Self>) {
        self.toasts.update(cx, |toasts, cx| toasts.dismiss(id, cx));
    }

    pub fn register_resources<'resource>(
        runtime: &mut impl ResourceRuntimeApi<'resource>,
    ) -> Result<(), FontRegistryError> {
        crate::typography::register(runtime)
    }
}

impl Render for InkPaperApp {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        let control_center_open = self.control_center.is_open();
        // read only while shown, so the app renders again for them only then
        let (battery, clock) = if control_center_open {
            let battery = self.battery.read(cx, |battery| battery.get());

            (battery, self.local_clock(cx))
        } else {
            (None, None)
        };

        let reader_title = if self.screen() == Screen::Reader {
            Some(self.reader.title())
        } else {
            None
        };

        rsx! {
            <div class="relative w-[480px] h-[800px] bg-white">
                {#if self.sleeping}
                    <SleepScreen />
                {:else}
                    {self.screen().route().render(self, cx)}
                {/if}

                {#if control_center_open}
                    <ControlCenter
                        battery={battery}
                        clock={clock}
                        reader_title={reader_title}
                        panel={self.frontlight_panel}
                    />
                {/if}

                {#if !self.sleeping}
                    {self.toasts}
                {/if}
            </div>
        }
    }
}
