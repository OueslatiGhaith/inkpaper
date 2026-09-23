use alloc::{format, string::String};

use inkpaper_ui::prelude::*;

use crate::{BatteryStatus, ClockStatus, FrontlightSetting};

const LIGHTBULB: SvgSource = include_svg!("assets/icons/lucide/lightbulb.svg");
const LIGHTBULB_OFF: SvgSource = include_svg!("assets/icons/lucide/lightbulb-off.svg");

#[component]
pub(crate) struct ControlCenter<'a> {
    battery: Option<BatteryStatus>,
    clock: Option<ClockStatus>,
    reader_title: Option<&'a str>,
    setting: FrontlightSetting,
}

impl RenderOnce for ControlCenter<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let brightness_label = format!("Brightness  {}%", self.setting.brightness());
        let warmth_label = format!("Warmth  {}%", self.setting.warmth());

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

        let brightness_fill = px(i32::from(self.setting.brightness()) * 280 / 100);
        let brightness_knob = px(8 + i32::from(self.setting.brightness()) * 266 / 100);

        let warmth_fill = px(i32::from(self.setting.warmth()) * 280 / 100);
        let warmth_knob = px(8 + i32::from(self.setting.warmth()) * 266 / 100);

        let lightbulb = if self.setting.is_on() {
            LIGHTBULB
        } else {
            LIGHTBULB_OFF
        };

        rsx! {
            <div class="absolute left-0 top-0 w-[480px] h-[800px]">
                // CrossInk top-anchored sheet. The quick-action bar is omitted
                // until those actions are real InkPaper features.
                <div class="absolute left-0 top-0 w-[480px] h-[382px] bg-white">
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

                    // CrossInk uses spaceLg * 2 = 32 px on each side.
                    //
                    // 105 = 5 top padding + 84 header + 16 leading space.
                    <div class="absolute left-8 top-[105px] w-[416px] flex flex-col">
                        // Brightness row: 56 px.
                        <div class="w-full h-14 flex items-center justify-between">
                            <text class="text-base no-wrap">
                                {brightness_label}
                            </text>

                            <div class="w-14 h-14 flex items-center justify-center">
                                {
                                    svg(lightbulb)
                                        .size(Size::new(px(28), px(28)))
                                        .text_color(Color::BLACK)
                                }
                            </div>
                        </div>

                        // CrossInk spaceSm.
                        <div class="h-1" />

                        <ControlCenterSliderRow
                            value={self.setting.brightness()}
                            fill_width={brightness_fill}
                            knob_left={brightness_knob}
                        />

                        // CrossInk spaceLg.
                        <div class="h-4" />

                        // Warmth uses one body-text line, then spaceSm.
                        <div class="w-full h-6 flex items-center">
                            <text class="text-base no-wrap">
                                {warmth_label}
                            </text>
                        </div>

                        <div class="h-1" />

                        <ControlCenterSliderRow
                            value={self.setting.warmth()}
                            fill_width={warmth_fill}
                            knob_left={warmth_knob}
                        />

                        // takeTop(..., spaceLg) + final screen.spacer(spaceLg).
                        <div class="h-8" />
                    </div>

                    // CrossInk's top sheet uses the FreeInk default rule and
                    // drawer handle, without Lyra corner rounding.
                    <div class="absolute left-0 bottom-0 w-full h-[2px] bg-black" />

                    <div class="absolute left-0 bottom-4 w-full flex justify-center">
                        <div class="w-[72px] h-[5px] rounded-md bg-black" />
                    </div>
                </div>
            </div>
        }
    }
}

#[component]
struct ControlCenterSliderRow {
    value: u8,
    fill_width: Pixels,
    knob_left: Pixels,
}

impl RenderOnce for ControlCenterSliderRow {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        rsx! {
            // 416 = 56 + 4 + 296 + 4 + 56, exactly matching CrossInk's
            // rowHeight/spaceSm geometry at the small UI scale.
            <div class="w-full h-14 flex items-center gap-1">
                <div class="w-14 h-14 flex items-center justify-center">
                    <text class="text-xl text-center">
                        "-"
                    </text>
                </div>

                <div class="relative w-[296px] h-14">
                    // FreeInk's normal slider has 8 px horizontal padding,
                    // a 4 px light-gray track, and a 14x22 black knob.
                    <div class="absolute left-2 top-[26px] w-[280px] h-1 bg-[#aaaaaa]" />

                    <div class="absolute left-2 top-[26px] w-{self.fill_width} h-1 bg-black" />

                    <div class="absolute left-{self.knob_left} top-[17px] w-[14px] h-[22px] bg-black" />
                </div>

                <div class="w-14 h-14 flex items-center justify-center">
                    <text class="text-xl text-center">
                        "+"
                    </text>
                </div>
            </div>
        }
    }
}
