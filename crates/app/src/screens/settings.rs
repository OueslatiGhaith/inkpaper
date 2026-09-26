use alloc::{format, string::String};
use inkpaper_ui::prelude::*;

use crate::{
    BatteryStatus, ClockStatus, InkPaperApp, UtcOffset,
    app::{ScreenInput, ScreenLifecycle, ScreenView},
    components::{
        header::{BackHeader, BackHeaderProps},
        settings_row::{
            ListRow, ListRowProps, SettingsToggleRow, SettingsToggleRowProps, SettingsValueRow,
            SettingsValueRowProps,
        },
        slider::{self, Slider, SliderProps},
    },
};

// screen geometry: the timezone block starts 683 px down, its slider after a
// 24 px label and a 4 px gap
const TIMEZONE_LEFT: i32 = 32;
const TIMEZONE_SLIDER_TOP: i32 = 683 + 24 + 4;

#[component]
pub(crate) struct SettingsScreen {
    battery: Option<BatteryStatus>,
    clock: Option<ClockStatus>,
    utc_offset: UtcOffset,
    on_back: Listener<ActivateEvent>,
    on_decrease_utc_offset: Listener<ActivateEvent>,
    on_increase_utc_offset: Listener<ActivateEvent>,
}

impl RenderOnce for SettingsScreen {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let clock = match self.clock {
            None => String::from("Date/time unavailable"),
            Some(clock) => format!(
                "{:02}/{:02}/{:04}  {:02}:{:02}",
                clock.day(),
                clock.month(),
                clock.year(),
                clock.hour(),
                clock.minute(),
            ),
        };

        let timezone_label = format!("Timezone  {}", self.utc_offset.label());

        rsx! {
            <div class="w-[480px] h-[800px] relative bg-white text-black">
                <div class="absolute left-0 top-[5px] w-[480px] h-[77px]">
                    <BackHeader
                        title="Settings"
                        battery={self.battery}
                        on_back={self.on_back}
                    />

                    <div class="absolute right-3 top-[32px]">
                        <text class="text-base">
                            {clock}
                        </text>
                    </div>
                </div>

                <div class="absolute left-0 top-[81px] w-[480px] h-[50px]">
                    <SettingsTabs />
                </div>

                <div class="absolute left-0 top-[147px] w-[480px] flex flex-col">
                    <ListRow
                        id={("settings-sleep-screen", 0)}
                        label="Sleep Screen"
                        depth={0}
                        selected={false}
                        chevron={true}
                        on_activate={None}
                    />

                    <SettingsToggleRow
                        label="Hide Battery %"
                        enabled={false}
                        selected={false}
                    />

                    <SettingsToggleRow
                        label="Hide Clock"
                        enabled={false}
                        selected={false}
                    />

                    <SettingsValueRow
                        label="Refresh Frequency"
                        value="5"
                        selected={false}
                    />

                    <SettingsToggleRow
                        label="Dark Mode"
                        enabled={false}
                        selected={false}
                    />

                    <SettingsValueRow
                        label="UI Theme"
                        value="Lyra"
                        selected={false}
                    />

                    <SettingsValueRow
                        label="UI Scale"
                        value="Small"
                        selected={false}
                    />

                    <SettingsValueRow
                        label="Recent Books View"
                        value="List View"
                        selected={false}
                    />
                </div>

                // inset like the reader menu's font size slider
                <div class="absolute left-8 top-[683px] w-[416px] flex flex-col">
                    <div class="w-full h-6 flex items-center">
                        <text class="text-base no-wrap">
                            {timezone_label}
                        </text>
                    </div>

                    <div class="h-1" />

                    <Slider
                        id="settings-timezone"
                        value={self.utc_offset.slider_value()}
                        on_decrease={Some(self.on_decrease_utc_offset)}
                        on_increase={Some(self.on_increase_utc_offset)}
                    />
                </div>
            </div>
        }
    }
}

#[component]
struct SettingsTabs;

impl RenderOnce for SettingsTabs {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let background = Color::rgb(170, 170, 170);

        rsx! {
            <div class="w-full h-full relative bg-{background}">
                <div class="absolute left-0 top-0 w-[120px] h-[49px] flex items-center justify-center">
                    <div class="w-[112px] h-[42px] rounded-md bg-black flex items-center justify-center">
                        <text class="text-base text-white">{"Display"}</text>
                    </div>
                </div>

                <div class="absolute left-[120px] top-0 w-[120px] h-[49px] flex items-center justify-center">
                    <text class="text-base">{"Reader"}</text>
                </div>

                <div class="absolute left-[240px] top-0 w-[120px] h-[49px] flex items-center justify-center">
                    <text class="text-base">{"Controls"}</text>
                </div>

                <div class="absolute left-[360px] top-0 w-[120px] h-[49px] flex items-center justify-center">
                    <text class="text-base">{"System"}</text>
                </div>

                <div class="absolute left-0 bottom-0 w-full h-px bg-black" />
            </div>
        }
    }
}

pub(crate) struct SettingsRoute;

impl ScreenLifecycle for SettingsRoute {}

impl ScreenInput for SettingsRoute {
    fn drag(
        &self,
        app: &mut InkPaperApp,
        origin: Point,
        position: Point,
        cx: &mut Context<'_, InkPaperApp>,
    ) -> bool {
        // dragging along the timezone slider previews an offset; release saves it
        if !slider::track_contains(origin, TIMEZONE_LEFT, TIMEZONE_SLIDER_TOP) {
            return false;
        }

        app.preview_utc_offset(timezone_at(position.x.get()), cx);

        true
    }

    fn release(
        &self,
        app: &mut InkPaperApp,
        position: Point,
        cx: &mut Context<'_, InkPaperApp>,
    ) -> bool {
        if slider::track_contains(position, TIMEZONE_LEFT, TIMEZONE_SLIDER_TOP) {
            app.set_utc_offset(timezone_at(position.x.get()), cx);

            return true;
        }

        // a drag that left the track still saves where it ended
        app.commit_utc_offset()
    }
}

fn timezone_at(x: i32) -> UtcOffset {
    UtcOffset::from_slider_value(slider::value_at(x, TIMEZONE_LEFT))
}

impl ScreenView for SettingsRoute {
    fn render<'a>(
        &self,
        app: &'a InkPaperApp,
        cx: &mut Context<'_, InkPaperApp>,
    ) -> AnyElement<'a> {
        SettingsScreen::from(SettingsScreenProps {
            battery: app.system_status.battery(),
            clock: app.local_clock(),
            utc_offset: app.clock.utc_offset(),
            on_back: cx.listener(InkPaperApp::activate_back),
            on_decrease_utc_offset: cx.listener(InkPaperApp::activate_decrease_utc_offset),
            on_increase_utc_offset: cx.listener(InkPaperApp::activate_increase_utc_offset),
        })
        .into_any_element()
    }
}
