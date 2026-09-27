use alloc::{format, string::String};
use inkpaper_ui::prelude::*;

use crate::{
    BatteryStatus, ClockStatus, InkPaperApp,
    app::{ScreenInput, ScreenLifecycle, ScreenView},
    components::{
        header::{BackHeader, BackHeaderProps},
        settings_row::{ListRow, ListRowProps},
    },
};

#[component]
pub(crate) struct SettingsScreen {
    battery: Option<BatteryStatus>,
    clock: Option<ClockStatus>,
    on_back: Listener<ActivateEvent>,
    on_clock: Listener<ActivateEvent>,
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

                <div class="absolute left-0 top-[98px] w-[480px]">
                    <ListRow
                        id={("settings-clock", 0)}
                        label="Clock"
                        depth={0}
                        selected={false}
                        chevron={true}
                        on_activate={Some(self.on_clock)}
                    />
                </div>
            </div>
        }
    }
}

pub(crate) struct SettingsRoute;

impl ScreenLifecycle for SettingsRoute {}

impl ScreenInput for SettingsRoute {}

impl ScreenView for SettingsRoute {
    fn render<'a>(
        &self,
        app: &'a InkPaperApp,
        cx: &mut Context<'_, InkPaperApp>,
    ) -> AnyElement<'a> {
        SettingsScreen::from(SettingsScreenProps {
            battery: app.system_status.battery(),
            clock: app.local_clock(),
            on_back: cx.listener(InkPaperApp::activate_back),
            on_clock: cx.listener(InkPaperApp::show_clock_settings),
        })
        .into_any_element()
    }
}
