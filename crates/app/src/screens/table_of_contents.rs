use inkpaper_ui::prelude::*;

use crate::{
    InkPaperApp,
    app::{Entry, ScreenInput, ScreenLifecycle, ScreenView, SideButton},
    components::{
        header::{BackHeader, BackHeaderProps, BatteryIndicator},
        page_number::{PageNumber, PageNumberProps},
        settings_row::{LIST_ROW_HEIGHT, ListRow, ListRowProps},
    },
    paging,
    reader::{TableOfContents, TocEntry},
};

/// The list fills the space under the header.
const LIST_HEIGHT: i32 = 682;
const ROWS_PER_PAGE: usize = (LIST_HEIGHT / LIST_ROW_HEIGHT) as usize;

/// Crosspoint's chapter selection: the book's table of contents, indented by
/// level, with the chapter being read highlighted.
#[component]
pub(crate) struct TableOfContentsScreen<'a> {
    toc: &'a TableOfContents,
    current: Option<usize>,
    page: usize,
    page_count: usize,
    on_entry: Listener<ActivateEvent>,
    battery: Entity<BatteryIndicator>,
    on_back: Listener<ActivateEvent>,
}

impl RenderOnce for TableOfContentsScreen<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let message = match self.toc {
            TableOfContents::NotLoaded | TableOfContents::Loading => Some("Loading chapters..."),
            TableOfContents::Failed => Some("Could not load chapters"),
            TableOfContents::Loaded(entries) if entries.is_empty() => Some("No chapters"),
            TableOfContents::Loaded(_) => None,
        };

        let entries = match self.toc {
            TableOfContents::Loaded(entries) => entries.as_slice(),
            _ => &[],
        };

        let list_height = px(LIST_HEIGHT);
        let paged = self.page_count > 1;

        rsx! {
            <div class="w-[480px] h-[800px] relative bg-white text-black">
                <div class="absolute left-0 top-[5px] w-[480px] h-[77px]">
                    <BackHeader
                        title="Select Chapter"
                        battery={self.battery}
                        on_back={self.on_back}
                    />
                </div>

                <div class="absolute left-0 top-[98px] w-[480px] h-{list_height}">
                    {#if let Some(message) = message}
                        <div class="w-full h-full flex items-center justify-center">
                            <text class="text-xl">
                                {message}
                            </text>
                        </div>
                    {:else}
                        <TocList
                            entries={entries}
                            current={self.current}
                            page={self.page}
                            on_entry={self.on_entry}
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

/// One page of the chapter list.
#[component]
struct TocList<'a> {
    entries: &'a [TocEntry],
    current: Option<usize>,
    page: usize,
    on_entry: Listener<ActivateEvent>,
}

impl RenderOnce for TocList<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let current = self.current;
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
            ListRow::from(ListRowProps {
                id: ("toc-entry", index),
                label: entry.label(),
                depth: entry.depth(),
                selected: current == Some(index),
                chevron: false,
                on_activate: Some(on_entry),
            })
        });

        div()
            .id(("toc-list", 0u8))
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .children(rows)
    }
}

pub(crate) struct TableOfContentsRoute;

impl ScreenLifecycle for TableOfContentsRoute {
    fn enter(&self, app: &mut InkPaperApp, entry: Entry) {
        // opens on the page holding the chapter being read
        if entry == Entry::Opened {
            app.reader.request_table_of_contents();
            app.reader.show_current_toc_page();
        }
    }
}

impl ScreenInput for TableOfContentsRoute {
    fn side_button(
        &self,
        app: &mut InkPaperApp,
        button: SideButton,
        cx: &mut Context<'_, InkPaperApp>,
    ) {
        paging::turn_page_by_button(app, button, cx, |app, turn| {
            app.reader.turn_toc_page(turn, ROWS_PER_PAGE)
        });
    }

    fn drag(
        &self,
        app: &mut InkPaperApp,
        origin: Point,
        position: Point,
        cx: &mut Context<'_, InkPaperApp>,
    ) {
        paging::turn_page_by_swipe(app, origin, position, cx, |app, turn| {
            app.reader.turn_toc_page(turn, ROWS_PER_PAGE)
        });
    }
}

impl ScreenView for TableOfContentsRoute {
    fn render<'a>(
        &self,
        app: &'a InkPaperApp,
        cx: &mut Context<'_, InkPaperApp>,
    ) -> AnyElement<'a> {
        TableOfContentsScreen::from(TableOfContentsScreenProps {
            toc: app.reader.table_of_contents(),
            current: app.reader.current_toc_index(),
            page: app.reader.toc_page(ROWS_PER_PAGE),
            page_count: app.reader.toc_page_count(ROWS_PER_PAGE),
            on_entry: cx.listener(InkPaperApp::activate_toc_entry),
            battery: app.battery_indicator,
            on_back: cx.listener(InkPaperApp::activate_back),
        })
        .into_any_element()
    }
}
