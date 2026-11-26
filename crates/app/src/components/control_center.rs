use alloc::{format, string::String};

use inkpaper_ui::prelude::*;

use crate::{
    BatteryStatus, ClockStatus, FrontlightState,
    components::{
        drawer_handle::{DrawerHandle, DrawerHandleProps},
        slider::{Slider, SliderProps, SliderScrubEvent},
    },
};

const LIGHTBULB: SvgSource = include_svg!("assets/icons/lucide/lightbulb.svg");
const LIGHTBULB_OFF: SvgSource = include_svg!("assets/icons/lucide/lightbulb-off.svg");

/// Marker event bound to the control center's handle band so swipe-up
/// hit-testing uses the handle's laid-out bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ControlCenterHandleEvent;

#[component]
pub(crate) struct ControlCenter<'a> {
    battery: Option<BatteryStatus>,
    clock: Option<ClockStatus>,
    reader_title: Option<&'a str>,
    panel: Entity<FrontlightPanel>,
    on_close: Listener<ActivateEvent>,
    on_swallow_activate: Listener<ActivateEvent>,
    on_swallow_scrub: Listener<SliderScrubEvent>,
    on_handle: Listener<ControlCenterHandleEvent>,
}

impl RenderOnce for ControlCenter<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let header_title = if let Some(title) = self.reader_title {
            if title.is_empty() {
                String::from("Frontlight")
            } else {
                String::from(title)
            }
        } else if let Some(clock) = self.clock {
            format!("{:02}/{:02}/{}", clock.day(), clock.month(), clock.year())
        } else {
            String::from("Frontlight")
        };

        let clock_label = self
            .clock
            .map(|clock| format!("{:02}:{:02}", clock.hour(), clock.minute()));

        let battery_label = self
            .battery
            .map(|battery| format!("{}%", battery.percent()))
            .unwrap_or_else(|| String::from("--%"));

        let battery_percent = self.battery.map(BatteryStatus::percent);

        let battery_low = battery_percent.is_some_and(|percent| percent > 10);
        let battery_mid = battery_percent.is_some_and(|percent| percent > 40);
        let battery_high = battery_percent.is_some_and(|percent| percent > 70);

        rsx! {
            <div
                id="control-center-overlay"
                on:SliderScrubEvent={self.on_swallow_scrub}
                class="absolute left-0 top-0 w-full h-full flex flex-col"
            >
                // Top-anchored sheet. The quick-action bar is omitted
                // until those actions are real InkPaper features.
                <div
                    id="control-center-sheet"
                    on:activate={self.on_swallow_activate}
                    class="relative w-full h-[382px] bg-white"
                >
                    // Lyra header: 5 px top padding, 84 px header.
                    <div class="absolute left-0 top-[5px] w-full h-[84px]">
                        {#if let Some(clock_label) = clock_label}
                            <div class="absolute left-3 top-0">
                                <text class="text-base no-wrap">
                                    {clock_label}
                                </text>
                            </div>
                        {/if}

                        <div class="absolute right-3 top-0 h-6 flex items-center gap-1">
                            <text class="text-base no-wrap">
                                {battery_label}
                            </text>

                            // Lyra-style 16x12 segmented battery.
                            <div class="relative w-4 h-3">
                                <div class="absolute left-0 top-0 w-[15px] h-3 border-px border-black" />
                                <div class="absolute left-[15px] top-[3px] w-px h-[6px] bg-black" />

                                {#if battery_low}
                                    <div class="absolute left-[2px] top-[2px] w-[3px] h-2 bg-black" />
                                {/if}

                                {#if battery_mid}
                                    <div class="absolute left-[6px] top-[2px] w-[3px] h-2 bg-black" />
                                {/if}

                                {#if battery_high}
                                    <div class="absolute left-[10px] top-[2px] w-[3px] h-2 bg-black" />
                                {/if}
                            </div>
                        </div>

                        <div class="absolute left-0 bottom-2 w-full h-6 flex items-center justify-center px-16">
                            <text class="text-base no-wrap max-lines-1 text-ellipsis text-center">
                                {header_title}
                            </text>
                        </div>

                        <div class="absolute left-0 bottom-0 w-full h-[3px] bg-black" />
                    </div>

                    {self.panel}

                    <div class="absolute left-0 bottom-0 w-full h-[2px] bg-black" />

                    <div
                        id="control-center-handle"
                        on:activate={self.on_close}
                        on:ControlCenterHandleEvent={self.on_handle}
                        class="absolute left-0 bottom-0 w-full h-8 flex items-center justify-center"
                    >
                        <DrawerHandle />
                    </div>
                </div>

                <div
                    id="control-center-backdrop"
                    on:activate={self.on_close}
                    class="w-full flex-1"
                />
            </div>
        }
    }
}

/// the frontlight controls of the control center. As its own entity, dragging a slider
/// renders and paints only these controls, not the screen under the control center
pub(crate) struct FrontlightPanel {
    frontlight: Entity<FrontlightState>,
}

impl FrontlightPanel {
    pub(crate) const fn new(frontlight: Entity<FrontlightState>) -> Self {
        Self { frontlight }
    }

    fn change(
        &mut self,
        cx: &mut Context<'_, Self>,
        update: impl FnOnce(&mut FrontlightState) -> bool,
    ) {
        self.frontlight.update(cx, |frontlight, cx| {
            if update(frontlight) {
                cx.notify();
            }
        });
    }

    fn toggle(&mut self, _: &ActivateEvent, cx: &mut Context<'_, Self>) {
        self.change(cx, FrontlightState::toggle);
    }

    fn decrease_brightness(&mut self, _: &ActivateEvent, cx: &mut Context<'_, Self>) {
        self.change(cx, |frontlight| frontlight.adjust_brightness(-1));
    }

    fn increase_brightness(&mut self, _: &ActivateEvent, cx: &mut Context<'_, Self>) {
        self.change(cx, |frontlight| frontlight.adjust_brightness(1));
    }

    fn scrub_brightness(&mut self, event: &SliderScrubEvent, cx: &mut Context<'_, Self>) {
        let value = event.value().max(1);
        self.change(cx, |frontlight| frontlight.set_brightness(value));
    }

    fn decrease_warmth(&mut self, _: &ActivateEvent, cx: &mut Context<'_, Self>) {
        self.change(cx, |frontlight| frontlight.adjust_warmth(-1));
    }

    fn increase_warmth(&mut self, _: &ActivateEvent, cx: &mut Context<'_, Self>) {
        self.change(cx, |frontlight| frontlight.adjust_warmth(1));
    }

    fn scrub_warmth(&mut self, event: &SliderScrubEvent, cx: &mut Context<'_, Self>) {
        let value = event.value();
        self.change(cx, |frontlight| frontlight.set_warmth(value));
    }
}

impl Render for FrontlightPanel {
    fn render(&mut self, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let setting = self.frontlight.read(cx).setting();
        let on_toggle = cx.listener(Self::toggle);
        let on_decrease_brightness = cx.listener(Self::decrease_brightness);
        let on_increase_brightness = cx.listener(Self::increase_brightness);
        let on_scrub_brightness = cx.listener(Self::scrub_brightness);
        let on_decrease_warmth = cx.listener(Self::decrease_warmth);
        let on_increase_warmth = cx.listener(Self::increase_warmth);
        let on_scrub_warmth = cx.listener(Self::scrub_warmth);

        let brightness_label = format!("Brightness  {}%", setting.brightness());
        let warmth_label = format!("Warmth  {}%", setting.warmth());

        let lightbulb = if setting.is_on() {
            LIGHTBULB
        } else {
            LIGHTBULB_OFF
        };

        rsx! {
            <div class="absolute left-8 top-[105px] w-[416px] flex flex-col">
                // Brightness row: 56 px.
                <div class="w-full h-14 flex items-center justify-between">
                    <text class="text-base no-wrap">
                        {brightness_label}
                    </text>

                    <div
                        id="control-center-frontlight-toggle"
                        on:activate={on_toggle}
                        class="w-14 h-14 flex items-center justify-center"
                    >
                        {
                            svg(lightbulb)
                                .size(Size::new(px(28), px(28)))
                                .text_color(Color::BLACK)
                        }
                    </div>
                </div>

                <div class="h-1" />

                <Slider
                    id="control-center-brightness"
                    value={setting.brightness()}
                    on_decrease={Some(on_decrease_brightness)}
                    on_increase={Some(on_increase_brightness)}
                    on_scrub={Some(on_scrub_brightness)}
                />

                <div class="h-4" />

                // Warmth uses one body-text line, then spaceSm.
                <div class="w-full h-6 flex items-center">
                    <text class="text-base no-wrap">
                        {warmth_label}
                    </text>
                </div>

                <div class="h-1" />

                <Slider
                    id="control-center-warmth"
                    value={setting.warmth()}
                    on_decrease={Some(on_decrease_warmth)}
                    on_increase={Some(on_increase_warmth)}
                    on_scrub={Some(on_scrub_warmth)}
                />

                // takeTop(..., spaceLg) + final screen.spacer(spaceLg).
                <div class="h-8" />
            </div>
        }
    }
}
