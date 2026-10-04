use inkpaper_ui::prelude::*;

use crate::{
    InkPaperApp,
    app::{Entry, ScreenInput, ScreenLifecycle, ScreenView},
    components::{
        header::{BackHeader, BackHeaderProps, BatteryIndicator},
        settings_row::{ListRow, ListRowProps},
    },
    reader::{TableOfContents, TocEntry},
};

/// Crosspoint's chapter selection: the book's table of contents, indented by
/// level, with the chapter being read highlighted.
#[component]
pub(crate) struct TableOfContentsScreen<'a> {
    toc: &'a TableOfContents,
    current: Option<usize>,
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

        rsx! {
            <div class="w-[480px] h-[800px] relative bg-white text-black">
                <div class="absolute left-0 top-[5px] w-[480px] h-[77px]">
                    <BackHeader
                        title="Select Chapter"
                        battery={self.battery}
                        on_back={self.on_back}
                    />
                </div>

                <div class="absolute left-0 top-[98px] w-[480px] h-[682px]">
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
                            on_entry={self.on_entry}
                        />
                    {/if}
                </div>
            </div>
        }
    }
}

#[component]
struct TocList<'a> {
    entries: &'a [TocEntry],
    current: Option<usize>,
    on_entry: Listener<ActivateEvent>,
}

impl RenderOnce for TocList<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let current = self.current;
        let on_entry = self.on_entry;

        // every row shares one listener; the row's id says which was tapped
        let rows = self.entries.iter().enumerate().map(move |(index, entry)| {
            ListRow::from(ListRowProps {
                id: ("toc-entry", index),
                label: entry.label(),
                depth: entry.depth(),
                selected: current == Some(index),
                chevron: false,
                on_activate: Some(on_entry),
            })
        });

        let list = div()
            .id(("toc-list", 0u8))
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .overflow_y_scroll();

        // open chapter selection with the current chapter at the top
        let list = match current {
            Some(index) => list.initial_scroll_to_child(index),
            None => list,
        };

        list.children(rows)
    }
}

pub(crate) struct TableOfContentsRoute;

impl ScreenLifecycle for TableOfContentsRoute {
    fn enter(&self, app: &mut InkPaperApp, entry: Entry) {
        if entry == Entry::Opened {
            app.reader.request_table_of_contents();
        }
    }
}

impl ScreenInput for TableOfContentsRoute {}

impl ScreenView for TableOfContentsRoute {
    fn render<'a>(
        &self,
        app: &'a InkPaperApp,
        cx: &mut Context<'_, InkPaperApp>,
    ) -> AnyElement<'a> {
        TableOfContentsScreen::from(TableOfContentsScreenProps {
            toc: app.reader.table_of_contents(),
            current: app.reader.current_toc_index(),
            on_entry: cx.listener(InkPaperApp::activate_toc_entry),
            battery: app.battery_indicator,
            on_back: cx.listener(InkPaperApp::activate_back),
        })
        .into_any_element()
    }
}
