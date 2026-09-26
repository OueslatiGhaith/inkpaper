use alloc::vec::Vec;

use inkpaper_ui::prelude::*;

use crate::{
    BatteryStatus, InkPaperApp,
    app::{Entry, ScreenInput, ScreenLifecycle, ScreenView},
    components::{
        header::{BackHeader, BackHeaderProps},
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
    entry_listeners: Vec<Listener<ActivateEvent>>,
    battery: Option<BatteryStatus>,
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
                            listeners={self.entry_listeners}
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
    listeners: Vec<Listener<ActivateEvent>>,
}

impl RenderOnce for TocList<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let current = self.current;

        let rows = self.entries.iter().zip(self.listeners).enumerate().map(
            move |(index, (entry, listener))| {
                ListRow::from(ListRowProps {
                    id: ("toc-entry", index),
                    label: entry.label(),
                    depth: entry.depth(),
                    selected: current == Some(index),
                    chevron: false,
                    on_activate: Some(listener),
                })
            },
        );

        div()
            .id(("toc-list", 0u8))
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .overflow_y_scroll()
            .children(rows)
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
        let toc = app.reader.table_of_contents();

        let entry_count = match toc {
            TableOfContents::Loaded(entries) => entries.len(),
            _ => 0,
        };

        let entry_listeners = (0..entry_count)
            .map(|index| {
                cx.listener(
                    move |app: &mut InkPaperApp,
                          _: &ActivateEvent,
                          cx: &mut Context<'_, InkPaperApp>| {
                        app.activate_toc_entry(index, cx);
                    },
                )
            })
            .collect();

        TableOfContentsScreen::from(TableOfContentsScreenProps {
            toc,
            current: app.reader.current_toc_index(),
            entry_listeners,
            battery: app.system_status.battery(),
            on_back: cx.listener(InkPaperApp::activate_back),
        })
        .into_any_element()
    }
}
