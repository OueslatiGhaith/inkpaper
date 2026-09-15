use inkpaper_ui::prelude::*;

use crate::components::{
    header::{BackHeader, BackHeaderProps},
    settings_row::{
        SettingsSubmenuRow, SettingsSubmenuRowProps, SettingsToggleRow, SettingsToggleRowProps,
        SettingsValueRow, SettingsValueRowProps,
    },
};

#[component]
pub(crate) struct SettingsScreen;

impl RenderOnce for SettingsScreen {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        rsx! {
            <div class="w-[480px] h-[800px] relative bg-white text-black">
                <div class="absolute left-0 top-[5px] w-[480px] h-[77px]">
                    <BackHeader
                        title="Settings"
                        battery="72%"
                        charging={false}
                    />

                    // CrossInk shows the synced date in the root Settings header.
                    // TODO: replace with RTC-backed date once app state is wired.
                    <div class="absolute right-3 top-6">
                        <text class="text-xl">{"15/09/2026"}</text>
                    </div>
                </div>

                <div class="absolute left-0 top-[81px] w-[480px] h-[50px]">
                    <SettingsTabs />
                </div>

                <div class="absolute left-0 top-[147px] w-[480px] flex flex-col">
                    <SettingsSubmenuRow
                        label="Sleep Screen"
                        selected={false}
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
