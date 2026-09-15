use inkpaper_ui::{FontRegistryError, prelude::*};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Screen {
    Home,
    BrowseFiles,
    RecentBooks,
    FileTransfer,
    Settings,
}

pub struct InkPaperApp {
    screen: Screen,
}

impl Default for InkPaperApp {
    fn default() -> Self {
        Self {
            screen: Screen::Home,
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

    fn show_browse_files(&mut self, _: &ActivateEvent, cx: &mut Context<'_, Self>) {
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
}

impl Render for InkPaperApp {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        let browse_files = cx.listener(Self::show_browse_files);
        let recent_books = cx.listener(Self::show_recent_books);
        let file_transfer = cx.listener(Self::show_file_transfer);
        let settings = cx.listener(Self::show_settings);
        let home = cx.listener(Self::show_home);

        rsx! {
            {#if self.screen == Screen::Home}
                <crate::screens::home::HomeScreen
                    on_browse_files={browse_files}
                    on_recent_books={recent_books}
                    on_file_transfer={file_transfer}
                    on_settings={settings}
                />
            {:else if self.screen == Screen::BrowseFiles}
                <crate::screens::browse_files::BrowseFilesScreen
                    on_back={home}
                />
            {:else if self.screen == Screen::RecentBooks}
                <crate::screens::recent_books::RecentBooksScreen
                    on_back={home}
                />
            {:else if self.screen == Screen::FileTransfer}
                <crate::screens::file_transfer::FileTransferScreen
                    on_back={home}
                />
            {:else}
                <crate::screens::settings::SettingsScreen
                    on_back={home}
                />
            {/if}
        }
    }
}
