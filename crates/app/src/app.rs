use alloc::{string::String, vec::Vec};
use inkpaper_epub::SpineIndex;
use inkpaper_ui::{FontRegistryError, prelude::*};

use crate::{
    BatteryStatus, BrowseListing, BrowseRequest, ClockStatus, FrontlightSetting, FrontlightState,
    ReaderChapter, ReaderChapterDirection, ReaderDocument, ReaderPreferences,
    ReaderPreferencesRequest, ReaderRequest, ReadingHistoryEntry, ReadingHistoryRequest,
    browser::BrowserState,
    components::control_center::{ControlCenter, ControlCenterProps},
    control_center::{
        ControlCenterAction, ControlCenterPointerResult, ControlCenterSlider, ControlCenterState,
    },
    input::PointerAction,
    reader::ReaderState,
    reader_page::paint_reader_page,
    reading_history::ReadingHistoryState,
    screens::{
        browse_files::{BrowseFilesScreen, BrowseFilesScreenProps},
        file_transfer::{FileTransferScreen, FileTransferScreenProps},
        home::{HomeScreen, HomeScreenProps},
        reader::{ReaderScreen, ReaderScreenProps},
        recent_books::{RecentBooksScreen, RecentBooksScreenProps},
        settings::{SettingsScreen, SettingsScreenProps},
    },
    system::SystemStatus,
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
    reading_history: ReadingHistoryState,
    system_status: SystemStatus,
    frontlight: FrontlightState,
    control_center: ControlCenterState,
    reader_return: Screen,
}

impl Default for InkPaperApp {
    fn default() -> Self {
        Self {
            screen: Screen::Home,
            browser: BrowserState::default(),
            reader: ReaderState::default(),
            reading_history: ReadingHistoryState::default(),
            system_status: SystemStatus::default(),
            frontlight: FrontlightState::default(),
            control_center: ControlCenterState::default(),
            reader_return: Screen::BrowseFiles,
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
        self.reading_history.request_load();
        self.navigate(Screen::Home, cx);
    }

    pub(crate) fn take_browse_request(&mut self) -> Option<BrowseRequest> {
        self.browser.take_request()
    }

    pub(crate) fn apply_browse_listing(
        &mut self,
        listing: BrowseListing,
        cx: &mut Context<'_, Self>,
    ) {
        self.browser.apply_listing(listing);
        cx.notify();
    }

    pub(crate) fn apply_browse_error(&mut self, cx: &mut Context<'_, Self>) {
        self.browser.apply_error();
        cx.notify();
    }

    fn show_browse_files(&mut self, _: &ActivateEvent, cx: &mut Context<'_, Self>) {
        self.browser.request_current_directory();
        self.navigate(Screen::BrowseFiles, cx);
    }

    fn show_recent_books(&mut self, _: &ActivateEvent, cx: &mut Context<'_, Self>) {
        self.reading_history.request_load();
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
        let target = self.reader_return;

        if matches!(target, Screen::Home | Screen::RecentBooks) {
            self.reading_history.request_load();
        }

        self.navigate(target, cx);
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

        self.reader_return = Screen::BrowseFiles;

        self.reader.open(path, title);

        self.navigate(Screen::Reader, cx);
    }

    pub(crate) fn take_reader_request(&mut self) -> Option<ReaderRequest> {
        self.reader.take_request()
    }

    pub(crate) fn apply_reader_document(
        &mut self,
        document: ReaderDocument,
        cx: &mut Context<'_, Self>,
    ) -> bool {
        let changed = self.reader.apply_document(document);

        if changed {
            cx.notify();
        }

        changed
    }

    pub(crate) fn apply_reader_chapter(
        &mut self,
        path: String,
        from: SpineIndex,
        direction: ReaderChapterDirection,
        chapter: ReaderChapter,
        cx: &mut Context<'_, Self>,
    ) -> bool {
        let changed = self.reader.apply_chapter(&path, from, direction, chapter);

        if changed {
            cx.notify();
        }

        changed
    }

    pub(crate) fn finish_reader_chapter_request(
        &mut self,
        path: String,
        from: SpineIndex,
        direction: ReaderChapterDirection,
    ) -> bool {
        self.reader.finish_chapter_request(&path, from, direction)
    }

    pub(crate) fn apply_reader_error(&mut self, path: String, cx: &mut Context<'_, Self>) -> bool {
        let changed = self.reader.apply_error(&path);

        if changed {
            cx.notify();
        }

        changed
    }

    fn activate_previous_reader_page(&mut self, _: &ActivateEvent, cx: &mut Context<'_, Self>) {
        self.reader_previous_page(cx);
    }

    fn activate_next_reader_page(&mut self, _: &ActivateEvent, cx: &mut Context<'_, Self>) {
        self.reader_next_page(cx);
    }

    pub fn reader_previous_page(&mut self, cx: &mut Context<'_, Self>) -> bool {
        if self.screen != Screen::Reader {
            return false;
        }

        if self.reader.previous_page() {
            cx.notify();
        }

        true
    }

    pub fn reader_next_page(&mut self, cx: &mut Context<'_, Self>) -> bool {
        if self.screen != Screen::Reader {
            return false;
        }

        if self.reader.next_page() {
            cx.notify();
        }

        true
    }

    fn activate_toggle_reader_controls(&mut self, _: &ActivateEvent, cx: &mut Context<'_, Self>) {
        self.reader_toggle_controls(cx);
    }

    fn reader_toggle_controls(&mut self, cx: &mut Context<'_, Self>) -> bool {
        if self.screen != Screen::Reader {
            return false;
        }

        if self.reader.toggle_controls() {
            cx.notify();
        }

        true
    }

    fn activate_current_book(&mut self, _: &ActivateEvent, cx: &mut Context<'_, Self>) {
        self.open_history_entry(0, Screen::Home, cx);
    }

    fn activate_recent_book(&mut self, index: usize, cx: &mut Context<'_, Self>) {
        self.open_history_entry(index, Screen::RecentBooks, cx);
    }

    fn open_history_entry(&mut self, index: usize, return_to: Screen, cx: &mut Context<'_, Self>) {
        let Some(entry) = self.reading_history.entry(index) else {
            return;
        };

        let path = String::from(entry.path());
        let title = String::from(entry.display_title());

        self.reader_return = return_to;
        self.reader.open(path, title);
        self.navigate(Screen::Reader, cx);
    }

    pub(crate) fn take_reading_history_request(&mut self) -> Option<ReadingHistoryRequest> {
        self.reading_history.take_request()
    }

    pub(crate) fn apply_reading_history(
        &mut self,
        entries: Vec<ReadingHistoryEntry>,
        cx: &mut Context<'_, Self>,
    ) {
        self.reading_history.apply_entries(entries);

        cx.notify();
    }

    pub(crate) fn apply_reading_history_error(&mut self, cx: &mut Context<'_, Self>) {
        self.reading_history.apply_error();

        cx.notify();
    }

    fn activate_decrease_reader_font_size(
        &mut self,
        _: &ActivateEvent,
        cx: &mut Context<'_, Self>,
    ) {
        self.reader_decrease_font_size(cx);
    }

    fn activate_increase_reader_font_size(
        &mut self,
        _: &ActivateEvent,
        cx: &mut Context<'_, Self>,
    ) {
        self.reader_increase_font_size(cx);
    }

    pub fn reader_decrease_font_size(&mut self, _: &mut Context<'_, Self>) -> bool {
        if self.screen != Screen::Reader {
            return false;
        }

        self.reader.decrease_font_size();

        true
    }

    pub fn reader_increase_font_size(&mut self, _: &mut Context<'_, Self>) -> bool {
        if self.screen != Screen::Reader {
            return false;
        }

        self.reader.increase_font_size();

        true
    }

    pub(crate) fn apply_reader_repagination(
        &mut self,
        path: String,
        spine: SpineIndex,
        font_size: u16,
        chapter: ReaderChapter,
        cx: &mut Context<'_, Self>,
    ) -> bool {
        let changed = self
            .reader
            .apply_repaginated_chapter(&path, spine, font_size, chapter);

        if changed {
            cx.notify();
        }

        changed
    }

    pub(crate) fn finish_reader_repagination_request(
        &mut self,
        path: String,
        spine: SpineIndex,
        font_size: u16,
    ) -> bool {
        self.reader
            .finish_repagination_request(&path, spine, font_size)
    }

    pub(crate) fn take_reader_preferences_request(&mut self) -> Option<ReaderPreferencesRequest> {
        self.reader.take_preferences_request()
    }

    pub(crate) fn apply_reader_preferences(
        &mut self,
        preferences: ReaderPreferences,
        cx: &mut Context<'_, Self>,
    ) -> bool {
        let changed = self.reader.apply_preferences(preferences);

        if changed {
            cx.notify();
        }

        changed
    }

    fn paint_reader_page(&self, paint: &mut PaintCx<'_>) {
        let Some(document) = self.reader.document() else {
            return;
        };

        let Some(page) = self.reader.page() else {
            return;
        };

        paint_reader_page(page, document, paint);
    }

    pub fn apply_battery_status(&mut self, battery: BatteryStatus, cx: &mut Context<'_, Self>) {
        let visible_changed = self.system_status.set_battery(battery);

        if visible_changed && (self.screen != Screen::Reader || self.control_center.is_open()) {
            cx.notify();
        }
    }

    pub fn apply_clock_status(&mut self, clock: Option<ClockStatus>, cx: &mut Context<'_, Self>) {
        let changed = self.system_status.set_clock(clock);

        // The clock is currently rendered only by Settings.
        //
        // Keeping RTC updates out of the Reader avoids waking the e-ink display
        // once per minute while somebody is reading.
        if changed && (self.screen == Screen::Settings || self.control_center.is_open()) {
            cx.notify();
        }
    }

    pub fn request_frontlight_apply(&mut self) {
        self.frontlight.request_apply();
    }

    pub(crate) fn take_frontlight_request(&mut self) -> Option<FrontlightSetting> {
        self.frontlight.take_request()
    }

    pub(crate) fn handle_previous_input(&mut self, cx: &mut Context<'_, Self>) -> bool {
        if self.control_center.is_open() {
            // CrossInk uses +/- 5 for the physical side buttons.
            if self.frontlight.adjust_brightness(-5) {
                cx.notify();
            }

            return true;
        }

        self.reader_previous_page(cx)
    }

    pub(crate) fn handle_next_input(&mut self, cx: &mut Context<'_, Self>) -> bool {
        if self.control_center.is_open() {
            if self.frontlight.adjust_brightness(5) {
                cx.notify();
            }

            return true;
        }

        self.reader_next_page(cx)
    }

    pub(crate) fn handle_home_input(&mut self, cx: &mut Context<'_, Self>) {
        if self.control_center.close() {
            cx.notify();
            return;
        }

        self.navigate_home(cx);
    }

    pub(crate) const fn allows_focus_navigation(&self) -> bool {
        !self.control_center.is_open()
    }

    pub(crate) const fn allows_wheel_scroll(&self) -> bool {
        !self.control_center.is_open()
    }

    pub(crate) const fn handle_confirm_down(&self) -> bool {
        self.control_center.is_open()
    }

    pub(crate) fn handle_confirm_up(&mut self, cx: &mut Context<'_, Self>) -> bool {
        if !self.control_center.is_open() {
            return false;
        }

        if self.frontlight.toggle() {
            cx.notify();
        }

        true
    }

    pub(crate) fn handle_pointer_down(
        &mut self,
        position: Point,
        cx: &mut Context<'_, Self>,
    ) -> PointerAction {
        let result = self.control_center.pointer_down(position);

        self.apply_control_center_pointer_result(result, PointerAction::Activate, cx)
    }

    pub(crate) fn handle_pointer_drag(
        &mut self,
        origin: Point,
        position: Point,
        cx: &mut Context<'_, Self>,
    ) -> PointerAction {
        let result = self.control_center.pointer_drag(origin, position);

        self.apply_control_center_pointer_result(result, PointerAction::Scroll, cx)
    }

    pub(crate) fn handle_pointer_up(
        &mut self,
        position: Point,
        cx: &mut Context<'_, Self>,
    ) -> PointerAction {
        let result = self.control_center.pointer_up(position);

        self.apply_control_center_pointer_result(result, PointerAction::Activate, cx)
    }

    pub(crate) fn cancel_pointer_input(&mut self) {
        self.control_center.cancel_pointer();
    }

    fn apply_control_center_pointer_result(
        &mut self,
        result: ControlCenterPointerResult,
        pass: PointerAction,
        cx: &mut Context<'_, Self>,
    ) -> PointerAction {
        match result {
            ControlCenterPointerResult::Pass => pass,

            ControlCenterPointerResult::Capture => PointerAction::Capture,

            ControlCenterPointerResult::Opened | ControlCenterPointerResult::Closed => {
                cx.notify();
                PointerAction::Capture
            }

            ControlCenterPointerResult::Slider { slider, value } => {
                let changed = match slider {
                    ControlCenterSlider::Brightness => self.frontlight.set_brightness(value),
                    ControlCenterSlider::Warmth => self.frontlight.set_warmth(value),
                };

                if changed {
                    cx.notify();
                }

                PointerAction::Capture
            }

            ControlCenterPointerResult::Action(action) => {
                let changed = match action {
                    ControlCenterAction::AdjustBrightness(delta) => {
                        self.frontlight.adjust_brightness(delta)
                    }

                    ControlCenterAction::AdjustWarmth(delta) => {
                        self.frontlight.adjust_warmth(delta)
                    }

                    ControlCenterAction::ToggleFrontlight => self.frontlight.toggle(),
                };

                if changed {
                    cx.notify();
                }

                PointerAction::Capture
            }
        }
    }
}

impl Render for InkPaperApp {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        let current_book = cx.listener(Self::activate_current_book);
        let browse_files = cx.listener(Self::show_browse_files);
        let recent_books = cx.listener(Self::show_recent_books);
        let file_transfer = cx.listener(Self::show_file_transfer);
        let settings = cx.listener(Self::show_settings);
        let home = cx.listener(Self::show_home);

        let browse_back = cx.listener(Self::browse_back);
        let reader_back = cx.listener(Self::reader_back);

        let reader_previous_page = cx.listener(Self::activate_previous_reader_page);
        let reader_next_page = cx.listener(Self::activate_next_reader_page);
        let reader_toggle_controls = cx.listener(Self::activate_toggle_reader_controls);

        let reader_decrease_font_size = cx.listener(Self::activate_decrease_reader_font_size);

        let reader_increase_font_size = cx.listener(Self::activate_increase_reader_font_size);

        let reader_page_canvas = if self.screen == Screen::Reader && self.reader.page().is_some() {
            Some(cx.canvas(|app, paint| app.paint_reader_page(paint)))
        } else {
            None
        };

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

        let recent_book_listeners = if self.screen == Screen::RecentBooks {
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

        let reader_title = if self.screen == Screen::Reader {
            Some(self.reader.title())
        } else {
            None
        };

        rsx! {
            <div class="relative w-[480px] h-[800px] bg-white">
                {#if self.screen == Screen::Home}
                    <HomeScreen
                        current_book={self.reading_history.current()}
                        battery={battery}
                        on_current_book={current_book}
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
                        battery={battery}
                        on_back={browse_back}
                    />
                {:else if self.screen == Screen::Reader}
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
                        on_back={reader_back}
                    />
                {:else if self.screen == Screen::RecentBooks}
                    <RecentBooksScreen
                        entries={self.reading_history.entries()}
                        entry_listeners={recent_book_listeners}
                        revision={self.reading_history.revision()}
                        error={self.reading_history.error()}
                        battery={battery}
                        on_back={home}
                    />
                {:else if self.screen == Screen::FileTransfer}
                    <FileTransferScreen
                        battery={battery}
                        on_back={home}
                    />
                {:else}
                    <SettingsScreen
                        battery={battery}
                        clock={clock}
                        on_back={home}
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
