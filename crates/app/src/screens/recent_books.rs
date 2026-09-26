use alloc::vec::Vec;

use inkpaper_ui::prelude::*;

use crate::{
    BatteryStatus, InkPaperApp, ReadingHistoryEntry,
    app::{Entry, ScreenInput, ScreenLifecycle},
    components::{
        header::{BackHeader, BackHeaderProps},
        recent_book_row::{RecentBookRow, RecentBookRowProps},
    },
};

#[component]
pub(crate) struct RecentBooksScreen<'a> {
    entries: &'a [ReadingHistoryEntry],
    entry_listeners: Vec<Listener<ActivateEvent>>,
    revision: u64,
    error: bool,
    battery: Option<BatteryStatus>,
    on_back: Listener<ActivateEvent>,
}

impl RenderOnce for RecentBooksScreen<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        rsx! {
            <div class="w-[480px] h-[800px] relative bg-white text-black">
                <div class="absolute left-0 top-[5px] w-[480px] h-[77px]">
                    <BackHeader
                        title="Recent Books"
                        battery={self.battery}
                        on_back={self.on_back}
                    />
                </div>

                <div class="absolute left-0 top-[98px] w-[480px] h-[682px]">
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
                            listeners={self.entry_listeners}
                            revision={self.revision}
                        />
                    {/if}
                </div>
            </div>
        }
    }
}

#[component]
struct RecentBookList<'a> {
    entries: &'a [ReadingHistoryEntry],
    listeners: Vec<Listener<ActivateEvent>>,
    revision: u64,
}

impl RenderOnce for RecentBookList<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let rows = self.entries.iter().zip(self.listeners).enumerate().map(
            |(index, (entry, listener))| {
                RecentBookRow::from(RecentBookRowProps {
                    id: index,
                    title: entry.display_title(),
                    subtitle: entry.display_subtitle(),
                    on_activate: listener,
                })
            },
        );

        div()
            .id(("recent-books-list", self.revision))
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .overflow_y_scroll()
            .children(rows)
    }
}

pub(crate) struct RecentBooksRoute;

impl ScreenLifecycle for RecentBooksRoute {
    fn enter(&self, app: &mut InkPaperApp, _: Entry) {
        app.reading_history.request_load();
    }
}

impl ScreenInput for RecentBooksRoute {}
