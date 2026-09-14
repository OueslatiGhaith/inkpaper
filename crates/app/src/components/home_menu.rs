use inkpaper_ui::prelude::*;

use crate::components::icon::{Icon, IconKind, IconProps};

#[component]
pub(crate) struct HomeMenu;

impl RenderOnce for HomeMenu {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        rsx! {
            <div class="w-full flex flex-col gap-1.5">
                <HomeMenuRow label="Browse Files" icon={IconKind::Folder} />
                <HomeMenuRow label="Recent Books" icon={IconKind::Recent} />
                <HomeMenuRow label="File Transfer" icon={IconKind::Transfer} />
                <HomeMenuRow label="Settings" icon={IconKind::Settings} />
            </div>
        }
    }
}

#[component]
struct HomeMenuRow {
    label: &'static str,
    icon: IconKind,
}

impl RenderOnce for HomeMenuRow {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        rsx! {
            <div class="w-full h-14 flex items-center pl-4 gap-[10px]">
                <Icon kind={self.icon} size={px(32)} />
                <text class="text-[12px] leading-4">{self.label}</text>
            </div>
        }
    }
}
