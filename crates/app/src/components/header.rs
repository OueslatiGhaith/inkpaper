use alloc::format;

use inkpaper_ui::prelude::*;

use crate::{
    BatteryStatus,
    components::icon::{Icon, IconKind, IconProps},
    system::BatteryState,
};

#[component]
pub(crate) struct HomeHeader {
    battery: Entity<BatteryIndicator>,
}

impl RenderOnce for HomeHeader {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        rsx! {
            <div class="w-full h-full relative">
                {self.battery}
            </div>
        }
    }
}

#[component]
pub(crate) struct TitleHeader<'a> {
    title: &'a str,
    battery: Entity<BatteryIndicator>,
}

impl RenderOnce for TitleHeader<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        rsx! {
            <div class="w-full h-full relative">
                {self.battery}

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
    battery: Entity<BatteryIndicator>,
    on_back: Listener<ActivateEvent>,
}

impl RenderOnce for BackHeader<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        rsx! {
            <div class="w-full h-full relative">
                {self.battery}

                <div
                    id="back"
                    on:activate={self.on_back}
                    class="absolute left-0 top-3 w-[52px] h-[52px] flex items-center justify-center"
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

/// the battery in a screen's header. As its own entity, a new percentage renders and
/// paints only the indicator, not the screen
pub(crate) struct BatteryIndicator {
    battery: Entity<BatteryState>,
}

impl BatteryIndicator {
    pub(crate) const fn new(battery: Entity<BatteryState>) -> Self {
        Self { battery }
    }
}

impl Render for BatteryIndicator {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        let battery = self
            .battery
            .read(cx, |battery| battery.get())
            .ok()
            .flatten();

        BatteryLabel::from(BatteryLabelProps { battery })
    }
}

#[component]
struct BatteryLabel {
    battery: Option<BatteryStatus>,
}

impl RenderOnce for BatteryLabel {
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
