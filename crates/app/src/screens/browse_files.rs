use alloc::format;
use inkpaper_ui::prelude::*;

use crate::{
    InkPaperApp,
    app::{Back, Entry, ScreenInput, ScreenLifecycle, ScreenView, SideButton},
    browser::{BrowseEntry, BrowseEntryKind, PageTurn},
    components::{
        file_row::{FILE_ROW_HEIGHT, FileKind, FileRow, FileRowProps},
        header::{BackHeader, BackHeaderProps, BatteryIndicator},
    },
};

/// The list fills the space between the header and the footer.
const LIST_HEIGHT: i32 = 662;
const ROWS_PER_PAGE: usize = (LIST_HEIGHT / FILE_ROW_HEIGHT) as usize;
const SWIPE_DISTANCE: i32 = 40;

#[component]
pub(crate) struct BrowseFilesScreen<'a> {
    title: &'a str,
    path: &'a str,
    entries: &'a [BrowseEntry],
    on_entry: Listener<ActivateEvent>,
    revision: u64,
    page: usize,
    page_count: usize,
    error: bool,
    battery: Entity<BatteryIndicator>,
    on_back: Listener<ActivateEvent>,
}

impl RenderOnce for BrowseFilesScreen<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let list_height = px(LIST_HEIGHT);
        let paged = self.page_count > 1;
        let page_label = format!("{}/{}", self.page + 1, self.page_count);
        let path_width = if paged { px(360) } else { px(440) };

        rsx! {
            <div class="w-[480px] h-[800px] relative bg-white text-black">
                <div class="absolute left-0 top-[5px] w-[480px] h-[77px]">
                    <BackHeader
                        title={self.title}
                        battery={self.battery}
                        on_back={self.on_back}
                    />
                </div>

                <div class="absolute left-0 top-[98px] w-[480px] h-{list_height}">
                    {#if self.error}
                        <div class="w-full h-full flex items-center justify-center">
                            <text class="text-xl">
                                {"Could not read directory"}
                            </text>
                        </div>
                    {:else if self.entries.is_empty()}
                        <div class="w-full h-full flex items-center justify-center">
                            <text class="text-xl">
                                {"This folder is empty"}
                            </text>
                        </div>
                    {:else}
                        <FileList
                            entries={self.entries}
                            on_entry={self.on_entry}
                            revision={self.revision}
                            page={self.page}
                        />
                    {/if}
                </div>

                <div class="absolute left-0 bottom-0 w-[480px] h-10">
                    <div class="absolute left-0 top-0 w-full h-[3px] bg-black" />

                    <div class="absolute left-5 top-3 w-{path_width}">
                        <text class="text-base no-wrap max-lines-1 text-ellipsis">
                            {self.path}
                        </text>
                    </div>

                    {#if paged}
                        <div class="absolute right-5 top-3">
                            <text class="text-base no-wrap">
                                {page_label}
                            </text>
                        </div>
                    {/if}
                </div>
            </div>
        }
    }
}

/// One page of the listing. Lists turn pages rather than scroll, since an
/// e-ink panel redraws every step of a scroll.
#[component]
struct FileList<'a> {
    entries: &'a [BrowseEntry],
    on_entry: Listener<ActivateEvent>,
    revision: u64,
    page: usize,
}

impl RenderOnce for FileList<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let on_entry = self.on_entry;

        let first = self.page * ROWS_PER_PAGE;

        // every row shares one listener; the row's id says which was tapped
        let rows = self
            .entries
            .iter()
            .enumerate()
            .skip(first)
            .take(ROWS_PER_PAGE);
        let rows = rows.map(move |(index, entry)| {
            let kind = match entry.kind() {
                BrowseEntryKind::Directory => FileKind::Folder,
                BrowseEntryKind::File => FileKind::File,
            };

            FileRow::from(FileRowProps {
                id: index,
                name: entry.display_name(),
                extension: entry.extension(),
                kind,
                on_activate: on_entry,
            })
        });

        div()
            .id(("browse-list", self.revision))
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .children(rows)
    }
}

pub(crate) struct BrowseFilesRoute;

impl ScreenLifecycle for BrowseFilesRoute {
    fn enter(&self, app: &mut InkPaperApp, entry: Entry) {
        // returning from the reader keeps the listing already shown
        if entry == Entry::Opened {
            app.browser.request_current_directory();
        }
    }

    fn back(&self, app: &mut InkPaperApp, cx: &mut Context<'_, InkPaperApp>) -> Back {
        if app.browser.request_parent() {
            cx.notify();
            return Back::Handled;
        }

        Back::Leave
    }
}

impl ScreenInput for BrowseFilesRoute {
    fn side_button(
        &self,
        app: &mut InkPaperApp,
        button: SideButton,
        cx: &mut Context<'_, InkPaperApp>,
    ) {
        let turn = match button {
            SideButton::Previous => PageTurn::Previous,
            SideButton::Next => PageTurn::Next,
        };

        if app.browser.turn_page(turn, ROWS_PER_PAGE) {
            cx.notify();
        }
    }

    // a vertical swipe turns one page: up for the next, down for the previous
    fn drag(
        &self,
        app: &mut InkPaperApp,
        origin: Point,
        position: Point,
        cx: &mut Context<'_, InkPaperApp>,
    ) -> bool {
        let dx = position.x.get() - origin.x.get();
        let dy = position.y.get() - origin.y.get();

        if dy.abs() < SWIPE_DISTANCE || dy.abs() <= dx.abs() {
            return false;
        }

        let turn = if dy < 0 {
            PageTurn::Next
        } else {
            PageTurn::Previous
        };

        if app.browser.turn_page(turn, ROWS_PER_PAGE) {
            cx.notify();
        }

        // the rest of the swipe turns nothing more
        app.capture_pointer();

        true
    }
}

impl ScreenView for BrowseFilesRoute {
    fn render<'a>(
        &self,
        app: &'a InkPaperApp,
        cx: &mut Context<'_, InkPaperApp>,
    ) -> AnyElement<'a> {
        BrowseFilesScreen::from(BrowseFilesScreenProps {
            title: app.browser.title(),
            path: app.browser.path(),
            entries: app.browser.entries(),
            on_entry: cx.listener(InkPaperApp::activate_browse_entry),
            revision: app.browser.revision(),
            page: app.browser.page(ROWS_PER_PAGE),
            page_count: app.browser.page_count(ROWS_PER_PAGE),
            error: app.browser.error(),
            battery: app.battery_indicator,
            on_back: cx.listener(InkPaperApp::activate_back),
        })
        .into_any_element()
    }
}
