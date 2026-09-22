use alloc::vec::Vec;

use inkpaper_ui::prelude::*;

use crate::{
    BatteryStatus,
    browser::{BrowseEntry, BrowseEntryKind},
    components::{
        file_row::{FileKind, FileRow, FileRowProps},
        header::{FileBrowserHeader, FileBrowserHeaderProps},
    },
};

#[component]
pub(crate) struct BrowseFilesScreen<'a> {
    title: &'a str,
    path: &'a str,
    entries: &'a [BrowseEntry],
    entry_listeners: Vec<Listener<ActivateEvent>>,
    revision: u64,
    error: bool,
    battery: Option<BatteryStatus>,
    on_back: Listener<ActivateEvent>,
}

impl RenderOnce for BrowseFilesScreen<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        rsx! {
            <div class="w-[480px] h-[800px] relative bg-white text-black">
                <div class="absolute left-0 top-[5px] w-[480px] h-[77px]">
                    <FileBrowserHeader
                        title={self.title}
                        battery={self.battery}
                        on_back={self.on_back}
                    />
                </div>

                <div class="absolute left-0 top-[98px] w-[480px] h-[662px]">
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
                            listeners={self.entry_listeners}
                            revision={self.revision}
                        />
                    {/if}
                </div>

                <div class="absolute left-0 bottom-0 w-[480px] h-10">
                    <div class="absolute left-0 top-0 w-full h-[3px] bg-black" />

                    <div class="absolute left-5 top-3 w-[440px]">
                        <text class="text-base no-wrap max-lines-1 text-ellipsis">
                            {self.path}
                        </text>
                    </div>
                </div>
            </div>
        }
    }
}

#[component]
struct FileList<'a> {
    entries: &'a [BrowseEntry],
    listeners: Vec<Listener<ActivateEvent>>,
    revision: u64,
}

impl RenderOnce for FileList<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let rows = self.entries.iter().zip(self.listeners).enumerate().map(
            |(index, (entry, listener))| {
                let kind = match entry.kind() {
                    BrowseEntryKind::Directory => FileKind::Folder,
                    BrowseEntryKind::File => FileKind::File,
                };

                FileRow::from(FileRowProps {
                    id: index,
                    name: entry.display_name(),
                    extension: entry.extension(),
                    kind,
                    on_activate: listener,
                })
            },
        );

        div()
            .id(("browse-list", self.revision))
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .overflow_y_scroll()
            .children(rows)
    }
}
