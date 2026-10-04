use inkpaper_ui::{FontRegistryError, prelude::*};

use crate::{
    ClockState, FileTransferState, FrontlightState,
    browser::BrowserState,
    components::control_center::{ControlCenter, ControlCenterProps},
    components::header::BatteryIndicator,
    control_center::ControlCenterState,
    reader::ReaderState,
    reading_history::ReadingHistoryState,
    screens::{
        reader::ReaderBatteryIcon,
        sleep::{SleepScreen, SleepScreenProps},
    },
    system::{BatteryState, RtcState},
    wifi::WifiState,
};

mod browse;
mod file_transfer;
mod history;
mod input;
mod navigation;
mod reader;
mod system;
mod wifi;

pub(crate) use input::{ScreenInput, SideButton};
use navigation::NavigationStack;
pub(crate) use navigation::{Back, Entry, Exit, ScreenLifecycle};

use crate::screens::{
    browse_files::BrowseFilesRoute, clock_settings::ClockSettingsRoute,
    file_transfer::FileTransferRoute, home::HomeRoute, reader::ReaderRoute,
    recent_books::RecentBooksRoute, settings::SettingsRoute,
    table_of_contents::TableOfContentsRoute, wifi_networks::WifiNetworksRoute,
    wifi_password::WifiPasswordRoute,
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
        }
    }
}

pub struct InkPaperApp {
    navigation: NavigationStack,
    sleeping: bool,
    pub(crate) browser: BrowserState,
    pub(crate) reader: ReaderState,
    pub(crate) reading_history: ReadingHistoryState,
    pub(crate) battery: Entity<BatteryState>,
    pub(crate) rtc: Entity<RtcState>,
    /// the battery in screen headers
    pub(crate) battery_indicator: Entity<BatteryIndicator>,
    /// the battery in the reader's status bar
    pub(crate) reader_battery_icon: Entity<ReaderBatteryIcon>,
    pub(crate) clock: ClockState,
    pub(crate) wifi: WifiState,
    frontlight: FrontlightState,
    control_center: ControlCenterState,
    /// set once a long press took the current touch
    pointer_captured: bool,
    pub(crate) file_transfer: FileTransferState,
}

impl InkPaperApp {
    pub fn new(cx: &mut Context<'_, Self>) -> Self {
        let battery = cx.new(|_| BatteryState::default()).unwrap();
        let rtc = cx.new(|_| RtcState::default()).unwrap();
        let battery_indicator = cx.new(|_| BatteryIndicator::new(battery)).unwrap();
        let reader_battery_icon = cx.new(|_| ReaderBatteryIcon::default()).unwrap();

        Self {
            navigation: NavigationStack::default(),
            sleeping: false,
            browser: BrowserState::default(),
            reader: ReaderState::default(),
            reading_history: ReadingHistoryState::default(),
            battery,
            rtc,
            battery_indicator,
            reader_battery_icon,
            clock: ClockState::default(),
            wifi: WifiState::default(),
            frontlight: FrontlightState::default(),
            control_center: ControlCenterState::default(),
            pointer_captured: false,
            file_transfer: FileTransferState::default(),
        }
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
            let battery = self
                .battery
                .read(cx, |battery| battery.get())
                .ok()
                .flatten();

            (battery, self.local_clock(cx))
        } else {
            (None, None)
        };
        let frontlight = self.frontlight.setting();

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
                        setting={frontlight}
                    />
                {/if}
            </div>
        }
    }
}
