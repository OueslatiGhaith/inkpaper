use inkpaper_ui::prelude::*;

use crate::components::icon::{Icon, IconKind, IconProps};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileKind {
    Folder,
    Book,
}

#[component]
pub(crate) struct FileRow<'a> {
    name: &'a str,
    extension: &'a str,
    kind: FileKind,
    selected: bool,
}

impl RenderOnce for FileRow<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let background = if self.selected {
            Color::rgb(170, 170, 170)
        } else {
            Color::WHITE
        };

        let icon = match self.kind {
            FileKind::Folder => IconKind::Folder,
            FileKind::Book => IconKind::BookMarked,
        };

        rsx! {
            <div class="w-full h-16 relative">
                <div class="absolute left-5 top-0 w-[440px] h-16 rounded-md bg-{background}" />

                <div class="absolute left-7 top-0 h-16 flex items-center">
                    <Icon kind={icon} size={px(24)} />
                </div>

                <div class="absolute left-[60px] top-0 w-[330px] h-16 flex items-center">
                    <text class="text-xl no-wrap max-lines-1 text-ellipsis">
                        {self.name}
                    </text>
                </div>

                <div class="absolute right-7 top-0 h-16 flex items-center">
                    <text class="text-xl no-wrap max-lines-1 text-clip">
                        {self.extension}
                    </text>
                </div>
            </div>
        }
    }
}
