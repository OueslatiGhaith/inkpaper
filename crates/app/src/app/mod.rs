use alloc::vec::Vec;
use inkpaper_ui::{FontRegistryError, prelude::*};

use crate::{
    FileTransferState, FrontlightState,
    browser::BrowserState,
    components::control_center::{ControlCenter, ControlCenterProps},
    control_center::ControlCenterState,
    reader::ReaderState,
    reading_history::ReadingHistoryState,
    screens::{
        browse_files::{BrowseFilesScreen, BrowseFilesScreenProps},
        file_transfer::{FileTransferScreen, FileTransferScreenProps},
        home::{HomeScreen, HomeScreenProps},
        reader::{ReaderScreen, ReaderScreenProps},
        recent_books::{RecentBooksScreen, RecentBooksScreenProps},
        settings::{SettingsScreen, SettingsScreenProps},
        sleep::{SleepScreen, SleepScreenProps},
    },
    system::SystemStatus,
};

mod browse;
mod file_transfer;
mod history;
mod input;
mod navigation;
mod reader;
mod system;

use browse::BrowseFilesRoute;
use file_transfer::FileTransferRoute;
use history::{HomeRoute, RecentBooksRoute};
use navigation::{NavigationStack, ScreenLifecycle};
use reader::ReaderRoute;
use system::SettingsRoute;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Screen {
    Home,
    BrowseFiles,
    Reader,
    RecentBooks,
    FileTransfer,
    Settings,
}

impl Screen {
    fn lifecycle(self) -> &'static dyn ScreenLifecycle {
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
    browser: BrowserState,
    reader: ReaderState,
    reading_history: ReadingHistoryState,
    system_status: SystemStatus,
    frontlight: FrontlightState,
    control_center: ControlCenterState,
    file_transfer: FileTransferState,
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
        let current_book = cx.listener(Self::activate_current_book);
        let browse_files = cx.listener(Self::show_browse_files);
        let recent_books = cx.listener(Self::show_recent_books);
        let file_transfer = cx.listener(Self::show_file_transfer);
        let settings = cx.listener(Self::show_settings);
        let back = cx.listener(Self::activate_back);

        let reader_previous_page = cx.listener(Self::activate_previous_reader_page);
        let reader_next_page = cx.listener(Self::activate_next_reader_page);
        let reader_toggle_controls = cx.listener(Self::activate_toggle_reader_controls);

        let reader_decrease_font_size = cx.listener(Self::activate_decrease_reader_font_size);

        let reader_increase_font_size = cx.listener(Self::activate_increase_reader_font_size);

        let usb_drive = cx.listener(Self::activate_usb_drive);

        let reader_page_canvas = if self.screen() == Screen::Reader && self.reader.page().is_some()
        {
            Some(cx.canvas(|app, paint| app.paint_reader_page(paint)))
        } else {
            None
        };

        let browse_entry_listeners = if self.screen() == Screen::BrowseFiles {
            (0..self.browser.entries().len())
                .map(|index| {
                    cx.listener(
                        move |app: &mut Self, _: &ActivateEvent, cx: &mut Context<'_, Self>| {
                            app.activate_browse_entry(index, cx);
                        },
                    )
                })
                .collect()
        } else {
            Vec::new()
        };

        let recent_book_listeners = if self.screen() == Screen::RecentBooks {
            (0..self.reading_history.entries().len())
                .map(|index| {
                    cx.listener(
                        move |app: &mut Self, _: &ActivateEvent, cx: &mut Context<'_, Self>| {
                            app.activate_recent_book(index, cx);
                        },
                    )
                })
                .collect()
        } else {
            Vec::new()
        };

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
                {:else if self.screen() == Screen::Home}
                    <HomeScreen
                        current_book={self.reading_history.current()}
                        battery={battery}
                        on_current_book={current_book}
                        on_browse_files={browse_files}
                        on_recent_books={recent_books}
                        on_file_transfer={file_transfer}
                        on_settings={settings}
                    />
                {:else if self.screen() == Screen::BrowseFiles}
                    <BrowseFilesScreen
                        title={self.browser.title()}
                        path={self.browser.path()}
                        entries={self.browser.entries()}
                        entry_listeners={browse_entry_listeners}
                        revision={self.browser.revision()}
                        error={self.browser.error()}
                        battery={battery}
                        on_back={back}
                    />
                {:else if self.screen() == Screen::Reader}
                    <ReaderScreen
                        title={self.reader.title()}
                        creator={self.reader.creator()}
                        status={self.reader.status()}
                        detail={self.reader.detail()}
                        path={self.reader.path()}
                        page_canvas={reader_page_canvas}
                        page_label={self.reader.page_label()}
                        section_label={self.reader.section_label()}
                        controls_visible={self.reader.controls_visible()}
                        font_size={self.reader.font_size()}
                        on_decrease_font_size={reader_decrease_font_size}
                        on_increase_font_size={reader_increase_font_size}
                        on_previous_page={reader_previous_page}
                        on_toggle_controls={reader_toggle_controls}
                        on_next_page={reader_next_page}
                        on_back={back}
                    />
                {:else if self.screen() == Screen::RecentBooks}
                    <RecentBooksScreen
                        entries={self.reading_history.entries()}
                        entry_listeners={recent_book_listeners}
                        revision={self.reading_history.revision()}
                        error={self.reading_history.error()}
                        battery={battery}
                        on_back={back}
                    />
                {:else if self.screen() == Screen::FileTransfer}
                    <FileTransferScreen
                        status={self.file_transfer.status()}
                        battery={battery}
                        on_back={back}
                        on_usb_drive={usb_drive}
                    />
                {:else if self.screen() == Screen::Settings}
                    <SettingsScreen
                        battery={battery}
                        clock={clock}
                        on_back={back}
                    />
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
