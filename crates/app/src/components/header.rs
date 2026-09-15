use inkpaper_ui::prelude::*;

use crate::components::icon::{Icon, IconKind, IconProps};

#[component]
pub(crate) struct HomeHeader<'a> {
    battery: &'a str,
    charging: bool,
}

impl RenderOnce for HomeHeader<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        rsx! {
            <div class="w-full h-full relative">
                <BatteryStatus battery={self.battery} charging={self.charging} />
            </div>
        }
    }
}

#[component]
pub(crate) struct BackHeader<'a> {
    title: &'a str,
    battery: &'a str,
    charging: bool,
    on_back: Listener<ActivateEvent>,
}

impl RenderOnce for BackHeader<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        rsx! {
            <div class="w-full h-full relative">
                <BatteryStatus battery={self.battery} charging={self.charging} />

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
    battery: &'a str,
    charging: bool,
    on_back: Listener<ActivateEvent>,
}

impl RenderOnce for FileBrowserHeader<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        rsx! {
            <div class="w-full h-full relative">
                <BackHeader
                    title={self.title}
                    battery={self.battery}
                    charging={self.charging}
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
struct BatteryStatus<'a> {
    battery: &'a str,
    charging: bool,
}

impl RenderOnce for BatteryStatus<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let icon = battery_icon(self.battery, self.charging);

        rsx! {
            <div class="absolute top-0 right-[18px] flex items-center gap-1">
                <text class="text-base">{self.battery}</text>
                <Icon kind={icon} size={px(24)} />
            </div>
        }
    }
}

fn battery_icon(battery: &str, charging: bool) -> IconKind {
    if charging {
        return IconKind::BatteryCharging;
    }

    match parse_battery_percent(battery) {
        0..=10 => IconKind::BatteryWarning,
        11..=35 => IconKind::BatteryLow,
        36..=70 => IconKind::BatteryMedium,
        71..=100 => IconKind::BatteryFull,
        _ => unreachable!(),
    }
}

fn parse_battery_percent(value: &str) -> u8 {
    let mut percent = 0u16;
    let mut has_digit = false;

    for byte in value.bytes() {
        if byte == b'%' {
            break;
        }

        if !byte.is_ascii_digit() {
            return 0;
        }

        has_digit = true;

        percent = percent
            .saturating_mul(10)
            .saturating_add(u16::from(byte - b'0'));
    }

    if !has_digit {
        return 0;
    }

    percent.min(100) as u8
}
