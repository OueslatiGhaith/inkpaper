use alloc::{format, string::String};
use inkpaper_ui::prelude::*;

use crate::{
    ClockStatus, ClockSyncStatus, InkPaperApp, UtcOffset,
    app::{Entry, ScreenInput, ScreenLifecycle, ScreenView},
    components::{
        header::{BackHeader, BackHeaderProps, BatteryIndicator},
        settings_row::{ListRow, ListRowProps, SettingsValueRow, SettingsValueRowProps},
        slider::{self, Slider, SliderProps},
    },
};

// screen geometry: the timezone block starts 186 px down, its slider after a
// 24 px label and a 4 px gap
const TIMEZONE_LEFT: i32 = 32;
const TIMEZONE_SLIDER_TOP: i32 = 186 + 24 + 4;

/// Clock settings, grouped like crosspoint's clock screen: the local time, the
/// timezone, and a network sync.
#[component]
pub(crate) struct ClockSettingsScreen<'a> {
    network: Option<&'a str>,
    battery: Entity<BatteryIndicator>,
    clock: Option<ClockStatus>,
    utc_offset: UtcOffset,
    sync: ClockSyncStatus,
    on_back: Listener<ActivateEvent>,
    on_decrease_utc_offset: Listener<ActivateEvent>,
    on_increase_utc_offset: Listener<ActivateEvent>,
    on_sync: Listener<ActivateEvent>,
    on_wifi: Listener<ActivateEvent>,
}

impl RenderOnce for ClockSettingsScreen<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let (time, date) = match self.clock {
            None => (String::from("--:--"), String::from("Time not set")),
            Some(clock) => (
                format!("{:02}:{:02}", clock.hour(), clock.minute()),
                format!(
                    "{:02}/{:02}/{:04}",
                    clock.day(),
                    clock.month(),
                    clock.year()
                ),
            ),
        };

        let timezone_label = format!("Timezone  {}", self.utc_offset.label());

        let status = match self.sync {
            ClockSyncStatus::Idle => String::from("Gets the time over WiFi"),
            ClockSyncStatus::Syncing => String::from("Syncing..."),
            ClockSyncStatus::Synced => String::from("Clock synced"),
            ClockSyncStatus::Failed(failure) => format!("Sync failed: {}", failure.message()),
        };

        // a running sync can't be started again
        let on_sync = (self.sync != ClockSyncStatus::Syncing).then_some(self.on_sync);

        rsx! {
            <div class="w-[480px] h-[800px] relative bg-white text-black">
                <div class="absolute left-0 top-[5px] w-[480px] h-[77px]">
                    <BackHeader
                        title="Clock"
                        battery={self.battery}
                        on_back={self.on_back}
                    />
                </div>

                <div class="absolute left-7 top-[98px] w-[424px] h-16 flex flex-col justify-center">
                    <text class="font-bold text-2xl no-wrap">
                        {time}
                    </text>

                    <text class="text-xl no-wrap">
                        {date}
                    </text>
                </div>

                // inset like the reader menu's font size slider
                <div class="absolute left-8 top-[186px] w-[416px] flex flex-col">
                    <div class="w-full h-6 flex items-center">
                        <text class="text-base no-wrap">
                            {timezone_label}
                        </text>
                    </div>

                    <div class="h-1" />

                    <Slider
                        id="clock-timezone"
                        value={self.utc_offset.slider_value()}
                        on_decrease={Some(self.on_decrease_utc_offset)}
                        on_increase={Some(self.on_increase_utc_offset)}
                    />
                </div>

                <div class="absolute left-0 top-[286px] w-[480px] flex flex-col">
                    <SettingsValueRow
                        id={("clock-wifi", 0)}
                        label="WiFi Network"
                        value={self.network.unwrap_or("Not connected")}
                        selected={false}
                        on_activate={Some(self.on_wifi)}
                    />

                    <ListRow
                        id={("clock-sync", 0)}
                        label="Sync Clock"
                        depth={0}
                        selected={false}
                        chevron={false}
                        on_activate={on_sync}
                    />
                </div>

                <div class="absolute left-7 top-[422px] w-[424px]">
                    <text class="text-base wrap max-lines-2 text-ellipsis">
                        {status}
                    </text>
                </div>
            </div>
        }
    }
}

pub(crate) struct ClockSettingsRoute;

impl ScreenLifecycle for ClockSettingsRoute {
    fn enter(&self, app: &mut InkPaperApp, entry: Entry) {
        if entry == Entry::Opened {
            app.clock.clear_sync_result();
        }
    }
}

impl ScreenInput for ClockSettingsRoute {
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

impl ScreenView for ClockSettingsRoute {
    fn render<'a>(
        &self,
        app: &'a InkPaperApp,
        cx: &mut Context<'_, InkPaperApp>,
    ) -> AnyElement<'a> {
        ClockSettingsScreen::from(ClockSettingsScreenProps {
            network: app.wifi.saved().connected().map(|network| network.ssid()),
            battery: app.battery_indicator,
            clock: app.local_clock(cx),
            utc_offset: app.clock.utc_offset(),
            sync: app.clock.sync_status(),
            on_back: cx.listener(InkPaperApp::activate_back),
            on_decrease_utc_offset: cx.listener(InkPaperApp::activate_decrease_utc_offset),
            on_increase_utc_offset: cx.listener(InkPaperApp::activate_increase_utc_offset),
            on_sync: cx.listener(InkPaperApp::activate_sync_clock),
            on_wifi: cx.listener(InkPaperApp::show_wifi_networks),
        })
        .into_any_element()
    }
}
