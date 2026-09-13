use inkpaper_ui::prelude::*;

use crate::components::icon::{Icon, IconKind, IconProps};

#[component]
pub(crate) struct HomeMenu;

impl RenderOnce for HomeMenu {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        rsx! {
            <div class="w-full flex flex-col gap-[6px]">
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
            <div class="w-full h-[56px] flex items-center pl-[16px] gap-[10px]">
                <Icon kind={self.icon} />
                <text class="text-[12px] leading-[16px]">{self.label}</text>
            </div>
        }
    }
}
