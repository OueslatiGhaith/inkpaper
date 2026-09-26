use alloc::format;

use inkpaper_ui::prelude::*;

use crate::{
    BatteryStatus,
    components::icon::{Icon, IconKind, IconProps},
};

#[component]
pub(crate) struct HomeHeader {
    battery: Option<BatteryStatus>,
}

impl RenderOnce for HomeHeader {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        rsx! {
            <div class="w-full h-full relative">
                <BatteryIndicator battery={self.battery} />
            </div>
        }
    }
}

#[component]
pub(crate) struct TitleHeader<'a> {
    title: &'a str,
    battery: Option<BatteryStatus>,
}

impl RenderOnce for TitleHeader<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        rsx! {
            <div class="w-full h-full relative">
                <BatteryIndicator battery={self.battery} />

                <div class="absolute left-5 top-3 h-[52px] flex items-center">
                    <text class="font-bold text-2xl no-wrap max-lines-1 text-ellipsis">
                        {self.title}
                    </text>
                </div>
            </div>
        }
    }
}

#[component]
pub(crate) struct BackHeader<'a> {
    title: &'a str,
    battery: Option<BatteryStatus>,
    on_back: Listener<ActivateEvent>,
}

impl RenderOnce for BackHeader<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        rsx! {
            <div class="w-full h-full relative">
                <BatteryIndicator battery={self.battery} />

                <div
                    id="back"
                    on:activate={self.on_back}
                    class="absolute left-0 top-3 w-[52px] h-[52px] flex items-center justify-center focus:bg-[#aaaaaa]"
                >
                    <Icon kind={IconKind::ChevronLeft} size={px(32)} />
                </div>

                <div class="absolute left-[60px] top-3 h-[52px] flex items-center">
                    <text class="font-bold text-2xl no-wrap max-lines-1 text-ellipsis">
                        {self.title}
                    </text>
                </div>
            </div>
        }
    }
}

#[component]
pub(crate) struct FileBrowserHeader<'a> {
    title: &'a str,
    battery: Option<BatteryStatus>,
    on_back: Listener<ActivateEvent>,
}

impl RenderOnce for FileBrowserHeader<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        rsx! {
            <div class="w-full h-full relative">
                <BackHeader
                    title={self.title}
                    battery={self.battery}
                    on_back={self.on_back}
                />

                <div class="absolute right-3.5 top-[26px] w-6 h-6">
                    <Icon kind={IconKind::SlidersHorizontal} size={px(24)} />
                </div>
            </div>
        }
    }
}

#[component]
struct BatteryIndicator {
    battery: Option<BatteryStatus>,
}

impl RenderOnce for BatteryIndicator {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let label = match self.battery {
            Some(battery) => format!("{}%", battery.percent()),
            None => format!("--%"),
        };

        let icon = self.battery.map(battery_icon);

        rsx! {
            <div class="absolute top-0 right-[18px] flex items-center gap-1">
                <text class="text-base">{label}</text>

                {#if let Some(icon) = icon}
                    <Icon kind={icon} size={px(24)} />
                {/if}
            </div>
        }
    }
}

pub(crate) fn battery_icon(battery: BatteryStatus) -> IconKind {
    match battery.percent() {
        0..=10 => IconKind::BatteryWarning,
        11..=35 => IconKind::BatteryLow,
        36..=70 => IconKind::BatteryMedium,
        71..=100 => IconKind::BatteryFull,
        _ => unreachable!(),
    }
}
