use alloc::vec::Vec;
use inkpaper_ui::{FontRegistryError, prelude::*};

use crate::{
    BrowseListing, BrowseRequest,
    browser::BrowserState,
    reader::ReaderState,
    screens::{
        browse_files::{BrowseFilesScreen, BrowseFilesScreenProps},
        file_transfer::{FileTransferScreen, FileTransferScreenProps},
        home::{HomeScreen, HomeScreenProps},
        reader::{ReaderScreen, ReaderScreenProps},
        recent_books::{RecentBooksScreen, RecentBooksScreenProps},
        settings::{SettingsScreen, SettingsScreenProps},
    },
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

pub struct InkPaperApp {
    screen: Screen,
    browser: BrowserState,
    reader: ReaderState,
}

impl Default for InkPaperApp {
    fn default() -> Self {
        Self {
            screen: Screen::Home,
            browser: BrowserState::default(),
            reader: ReaderState::default(),
        }
    }
}

impl InkPaperApp {
    pub fn register_resources<'resource>(
        runtime: &mut impl ResourceRuntimeApi<'resource>,
    ) -> Result<(), FontRegistryError> {
        crate::typography::register(runtime)
    }

    fn navigate(&mut self, screen: Screen, cx: &mut Context<'_, Self>) {
        if self.screen == screen {
            return;
        }

        self.screen = screen;
        cx.notify();
    }

    pub fn navigate_home(&mut self, cx: &mut Context<'_, Self>) {
        self.navigate(Screen::Home, cx);
    }

    pub fn take_browse_request(&mut self) -> Option<BrowseRequest> {
        self.browser.take_request()
    }

    pub fn apply_browse_listing(&mut self, listing: BrowseListing, cx: &mut Context<'_, Self>) {
        self.browser.apply_listing(listing);
        cx.notify();
    }

    pub fn apply_browse_error(&mut self, cx: &mut Context<'_, Self>) {
        self.browser.apply_error();
        cx.notify();
    }

    fn show_browse_files(&mut self, _: &ActivateEvent, cx: &mut Context<'_, Self>) {
        self.browser.request_current_directory();
        self.navigate(Screen::BrowseFiles, cx);
    }

    fn show_recent_books(&mut self, _: &ActivateEvent, cx: &mut Context<'_, Self>) {
        self.navigate(Screen::RecentBooks, cx);
    }

    fn show_file_transfer(&mut self, _: &ActivateEvent, cx: &mut Context<'_, Self>) {
        self.navigate(Screen::FileTransfer, cx);
    }

    fn show_settings(&mut self, _: &ActivateEvent, cx: &mut Context<'_, Self>) {
        self.navigate(Screen::Settings, cx);
    }

    fn show_home(&mut self, _: &ActivateEvent, cx: &mut Context<'_, Self>) {
        self.navigate_home(cx);
    }

    fn browse_back(&mut self, _: &ActivateEvent, cx: &mut Context<'_, Self>) {
        if self.browser.request_parent() {
            cx.notify();
        } else {
            self.navigate_home(cx);
        }
    }

    fn reader_back(&mut self, _: &ActivateEvent, cx: &mut Context<'_, Self>) {
        self.navigate(Screen::BrowseFiles, cx);
    }

    fn activate_browse_entry(&mut self, index: usize, cx: &mut Context<'_, Self>) {
        if self.browser.request_entry(index) {
            cx.notify();
            return;
        }

        let Some(file) = self.browser.file_at(index) else {
            return;
        };

        // EPUB is the first reader format. Other file types remain visible in Browse Files
        // but intentionally do nothing until their corresponding reader/viewer exists.
        if !file.is_epub() {
            return;
        }

        let (path, title) = file.into_reader_parts();

        self.reader.open(path, title);

        self.navigate(Screen::Reader, cx);
    }
}

impl Render for InkPaperApp {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        let browse_files = cx.listener(Self::show_browse_files);

        let recent_books = cx.listener(Self::show_recent_books);

        let file_transfer = cx.listener(Self::show_file_transfer);

        let settings = cx.listener(Self::show_settings);

        let home = cx.listener(Self::show_home);

        let browse_back = cx.listener(Self::browse_back);

        let reader_back = cx.listener(Self::reader_back);

        let browse_entry_listeners = if self.screen == Screen::BrowseFiles {
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

        rsx! {
            {#if self.screen == Screen::Home}
                <HomeScreen
                    on_browse_files={browse_files}
                    on_recent_books={recent_books}
                    on_file_transfer={file_transfer}
                    on_settings={settings}
                />
            {:else if self.screen == Screen::BrowseFiles}
                <BrowseFilesScreen
                    title={self.browser.title()}
                    path={self.browser.path()}
                    entries={self.browser.entries()}
                    entry_listeners={browse_entry_listeners}
                    revision={self.browser.revision()}
                    error={self.browser.error()}
                    on_back={browse_back}
                />
            {:else if self.screen == Screen::Reader}
                <ReaderScreen
                    title={self.reader.title()}
                    path={self.reader.path()}
                    on_back={reader_back}
                />
            {:else if self.screen == Screen::RecentBooks}
                <RecentBooksScreen
                    on_back={home}
                />
            {:else if self.screen == Screen::FileTransfer}
                <FileTransferScreen
                    on_back={home}
                />
            {:else}
                <SettingsScreen
                    on_back={home}
                />
            {/if}
        }
    }
}
