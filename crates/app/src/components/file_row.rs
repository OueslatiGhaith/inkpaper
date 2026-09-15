use inkpaper_ui::prelude::*;

use crate::components::icon::{Icon, IconKind, IconProps};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileKind {
    Folder,
    File,
}

#[component]
pub(crate) struct FileRow<'a> {
    id: usize,
    name: &'a str,
    extension: &'a str,
    kind: FileKind,
    on_activate: Listener<ActivateEvent>,
}

impl RenderOnce for FileRow<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let icon = match self.kind {
            FileKind::Folder => IconKind::Folder,
            FileKind::File => IconKind::BookMarked,
        };

        rsx! {
            <div
                id={("file-row", self.id)}
                on:activate={self.on_activate}
                class="
                    ml-5 w-[440px] h-16 relative rounded-md
                    focus:bg-[#aaaaaa]
                "
            >
                <div class="absolute left-2 top-0 h-16 flex items-center">
                    <Icon kind={icon} size={px(24)} />
                </div>

                <div class="absolute left-10 top-0 w-[330px] h-16 flex items-center">
                    <text class="text-xl no-wrap max-lines-1 text-ellipsis">
                        {self.name}
                    </text>
                </div>

                <div class="absolute right-2 top-0 h-16 flex items-center">
                    <text class="text-xl no-wrap max-lines-1 text-clip">
                        {self.extension}
                    </text>
                </div>
            </div>
        }
    }
}
