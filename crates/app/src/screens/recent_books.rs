use inkpaper_ui::prelude::*;

use crate::{
    InkPaperApp, ReadingHistoryEntry,
    app::{Entry, ScreenInput, ScreenLifecycle, ScreenView, SideButton},
    components::{
        header::{BackHeader, BackHeaderProps, BatteryIndicator},
        page_number::{PageNumber, PageNumberProps},
        recent_book_row::{RECENT_BOOK_ROW_HEIGHT, RecentBookRow, RecentBookRowProps},
    },
    paging,
};

/// The list fills the space under the header.
const LIST_HEIGHT: i32 = 682;
const ROWS_PER_PAGE: usize = (LIST_HEIGHT / RECENT_BOOK_ROW_HEIGHT) as usize;

#[component]
pub(crate) struct RecentBooksScreen<'a> {
    entries: &'a [ReadingHistoryEntry],
    on_entry: Listener<ActivateEvent>,
    revision: u64,
    page: usize,
    page_count: usize,
    error: bool,
    battery: Entity<BatteryIndicator>,
    on_back: Listener<ActivateEvent>,
}

impl RenderOnce for RecentBooksScreen<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let list_height = px(LIST_HEIGHT);
        let paged = self.page_count > 1;

        rsx! {
            <div class="w-[480px] h-[800px] relative bg-white text-black">
                <div class="absolute left-0 top-[5px] w-[480px] h-[77px]">
                    <BackHeader
                        title="Recent Books"
                        battery={self.battery}
                        on_back={self.on_back}
                    />
                </div>

                <div class="absolute left-0 top-[98px] w-[480px] h-{list_height}">
                    {#if self.error}
                        <div class="w-full h-full flex items-center justify-center">
                            <text class="text-xl">
                                {"Could not load recent books"}
                            </text>
                        </div>

                    {:else if self.entries.is_empty()}
                        <div class="w-full h-full flex items-center justify-center">
                            <text class="text-xl">
                                {"No recent books yet"}
                            </text>
                        </div>

                    {:else}
                        <RecentBookList
                            entries={self.entries}
                            on_entry={self.on_entry}
                            revision={self.revision}
                            page={self.page}
                        />
                    {/if}
                </div>

                {#if paged}
                    <div class="absolute right-5 bottom-3">
                        <PageNumber page={self.page} page_count={self.page_count} />
                    </div>
                {/if}
            </div>
        }
    }
}

/// One page of the history.
#[component]
struct RecentBookList<'a> {
    entries: &'a [ReadingHistoryEntry],
    on_entry: Listener<ActivateEvent>,
    revision: u64,
    page: usize,
}

impl RenderOnce for RecentBookList<'_> {
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
            RecentBookRow::from(RecentBookRowProps {
                id: index,
                title: entry.display_title(),
                subtitle: entry.display_subtitle(),
                on_activate: on_entry,
            })
        });

        div()
            .id(("recent-books-list", self.revision))
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .children(rows)
    }
}

pub(crate) struct RecentBooksRoute;

impl ScreenLifecycle for RecentBooksRoute {
    fn enter(&self, app: &mut InkPaperApp, _: Entry) {
        app.reading_history.request_load();
    }
}

impl ScreenInput for RecentBooksRoute {
    fn side_button(
        &self,
        app: &mut InkPaperApp,
        button: SideButton,
        cx: &mut Context<'_, InkPaperApp>,
    ) {
        paging::turn_page_by_button(app, button, cx, |app, turn| {
            app.reading_history.turn_page(turn, ROWS_PER_PAGE)
        });
    }

    fn drag(
        &self,
        app: &mut InkPaperApp,
        origin: Point,
        position: Point,
        cx: &mut Context<'_, InkPaperApp>,
    ) -> bool {
        paging::turn_page_by_swipe(app, origin, position, cx, |app, turn| {
            app.reading_history.turn_page(turn, ROWS_PER_PAGE)
        })
    }
}

impl ScreenView for RecentBooksRoute {
    fn render<'a>(
        &self,
        app: &'a InkPaperApp,
        cx: &mut Context<'_, InkPaperApp>,
    ) -> AnyElement<'a> {
        RecentBooksScreen::from(RecentBooksScreenProps {
            entries: app.reading_history.entries(),
            on_entry: cx.listener(InkPaperApp::activate_recent_book),
            revision: app.reading_history.revision(),
            page: app.reading_history.page(ROWS_PER_PAGE),
            page_count: app.reading_history.page_count(ROWS_PER_PAGE),
            error: app.reading_history.error(),
            battery: app.battery_indicator,
            on_back: cx.listener(InkPaperApp::activate_back),
        })
        .into_any_element()
    }
}
