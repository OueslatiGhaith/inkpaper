use inkpaper_ui::{FontRegistryError, prelude::*};

use crate::{
    FileTransferState, FrontlightState,
    browser::BrowserState,
    components::control_center::{ControlCenter, ControlCenterProps},
    control_center::ControlCenterState,
    reader::ReaderState,
    reading_history::ReadingHistoryState,
    screens::sleep::{SleepScreen, SleepScreenProps},
    system::SystemStatus,
};

mod browse;
mod file_transfer;
mod history;
mod input;
mod navigation;
mod reader;
mod system;

pub(crate) use input::{ScreenInput, SideButton};
use navigation::NavigationStack;
pub(crate) use navigation::{Back, Entry, Exit, ScreenLifecycle};

use crate::screens::{
    browse_files::BrowseFilesRoute, file_transfer::FileTransferRoute, home::HomeRoute,
    reader::ReaderRoute, recent_books::RecentBooksRoute, settings::SettingsRoute,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Screen {
    Home,
    BrowseFiles,
    Reader,
    RecentBooks,
    FileTransfer,
    Settings,
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
        }
    }
}

#[derive(Default)]
pub struct InkPaperApp {
    navigation: NavigationStack,
    sleeping: bool,
    pub(crate) browser: BrowserState,
    pub(crate) reader: ReaderState,
    pub(crate) reading_history: ReadingHistoryState,
    pub(crate) system_status: SystemStatus,
    frontlight: FrontlightState,
    control_center: ControlCenterState,
    pub(crate) file_transfer: FileTransferState,
}

impl InkPaperApp {
    pub fn register_resources<'resource>(
        runtime: &mut impl ResourceRuntimeApi<'resource>,
    ) -> Result<(), FontRegistryError> {
        crate::typography::register(runtime)
    }
}

impl Render for InkPaperApp {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        let battery = self.system_status.battery();
        let clock = self.system_status.clock();
        let control_center_open = self.control_center.is_open();
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
