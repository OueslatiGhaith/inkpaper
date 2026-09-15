use inkpaper_ui::prelude::*;

use crate::components::icon::{Icon, IconKind, IconProps};

#[component]
pub(crate) struct HomeMenu {
    on_browse_files: Listener<ActivateEvent>,
    on_recent_books: Listener<ActivateEvent>,
    on_file_transfer: Listener<ActivateEvent>,
    on_settings: Listener<ActivateEvent>,
}

impl RenderOnce for HomeMenu {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        rsx! {
            <div class="w-full flex flex-col gap-1.5">
                <HomeMenuRow
                    id="home-browse-files"
                    label="Browse Files"
                    icon={IconKind::Folder}
                    on_activate={self.on_browse_files}
                />
                <HomeMenuRow
                    id="home-recent-books"
                    label="Recent Books"
                    icon={IconKind::Recent}
                    on_activate={self.on_recent_books}
                />
                <HomeMenuRow
                    id="home-file-transfer"
                    label="File Transfer"
                    icon={IconKind::Transfer}
                    on_activate={self.on_file_transfer}
                />
                <HomeMenuRow
                    id="home-settings"
                    label="Settings"
                    icon={IconKind::Settings}
                    on_activate={self.on_settings}
                />
            </div>
        }
    }
}

#[component]
struct HomeMenuRow {
    id: &'static str,
    label: &'static str,
    icon: IconKind,
    on_activate: Listener<ActivateEvent>,
}

impl RenderOnce for HomeMenuRow {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        rsx! {
            <div
                id={self.id}
                on:activate={self.on_activate}
                class="w-full h-14 flex items-center pl-4 gap-2.5 focus:bg-[#aaaaaa]"
            >
                <Icon kind={self.icon} size={px(32)} />
                <text class="text-2xl">{self.label}</text>
            </div>
        }
    }
}
