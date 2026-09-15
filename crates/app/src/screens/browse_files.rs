use inkpaper_ui::prelude::*;

use crate::components::{
    file_row::{FileKind, FileRow, FileRowProps},
    header::{FileBrowserHeader, FileBrowserHeaderProps},
};

#[component]
pub(crate) struct BrowseFilesScreen;

impl RenderOnce for BrowseFilesScreen {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        rsx! {
            <div class="w-[480px] h-[800px] relative bg-white text-black">
                <div class="absolute left-0 top-[5px] w-[480px] h-[77px]">
                    <FileBrowserHeader title="SD Card" battery="72%" charging={false} />
                </div>

                <div class="absolute left-0 top-[98px] w-[480px] flex flex-col">
                    <FileRow
                        name="Books"
                        extension=""
                        kind={FileKind::Folder}
                        selected={true}
                    />
                    <FileRow
                        name="Documents"
                        extension=""
                        kind={FileKind::Folder}
                        selected={false}
                    />
                    <FileRow
                        name="Read"
                        extension=""
                        kind={FileKind::Folder}
                        selected={false}
                    />
                    <FileRow
                        name="Dune"
                        extension=".epub"
                        kind={FileKind::Book}
                        selected={false}
                    />
                    <FileRow
                        name="Project Hail Mary"
                        extension=".epub"
                        kind={FileKind::Book}
                        selected={false}
                    />
                    <FileRow
                        name="The Three-Body Problem"
                        extension=".epub"
                        kind={FileKind::Book}
                        selected={false}
                    />
                    <FileRow
                        name="The Left Hand of Darkness"
                        extension=".epub"
                        kind={FileKind::Book}
                        selected={false}
                    />
                </div>

                <div class="absolute left-0 bottom-0 w-[480px] h-10">
                    <div class="absolute left-0 top-0 w-full h-[3px] bg-black" />

                    <div class="absolute left-5 top-3">
                        <text class="text-base no-wrap max-lines-1 text-clip">"/"</text>
                    </div>
                </div>
            </div>
        }
    }
}
