use inkpaper_ui::prelude::*;

use crate::components::icon::{Icon, IconKind};

pub(crate) struct HomeMenuProps;

pub(crate) struct HomeMenu;

impl From<HomeMenuProps> for HomeMenu {
    fn from(_: HomeMenuProps) -> Self {
        Self
    }
}

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

struct HomeMenuRowProps {
    label: &'static str,
    icon: IconKind,
}

struct HomeMenuRow {
    label: &'static str,
    icon: IconKind,
}

impl From<HomeMenuRowProps> for HomeMenuRow {
    fn from(value: HomeMenuRowProps) -> Self {
        Self {
            label: value.label,
            icon: value.icon,
        }
    }
}

impl RenderOnce for HomeMenuRow {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let icon = Icon::new(self.icon);

        rsx! {
            <div class="w-full h-[56px] flex items-center pl-[16px] gap-[10px]">
                {icon}
                <text class="text-[12px] leading-[16px]">{self.label}</text>
            </div>
        }
    }
}
