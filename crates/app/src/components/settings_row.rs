use inkpaper_ui::prelude::*;

use crate::components::icon::{Icon, IconKind, IconProps};

fn row_background(selected: bool) -> Color {
    if selected {
        Color::rgb(170, 170, 170)
    } else {
        Color::WHITE
    }
}

#[component]
pub(crate) struct SettingsSubmenuRow<'a> {
    label: &'a str,
    selected: bool,
}

impl RenderOnce for SettingsSubmenuRow<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let background = row_background(self.selected);

        rsx! {
            <div class="w-full h-16 relative">
                <div class="absolute left-5 top-0 w-[440px] h-16 rounded-md bg-{background}" />

                <div class="absolute left-7 top-0 w-[380px] h-16 flex items-center">
                    <text class="text-xl no-wrap max-lines-1 text-ellipsis">
                        {self.label}
                    </text>
                </div>

                <div class="absolute right-7 top-0 h-16 flex items-center">
                    <Icon kind={IconKind::ChevronRight} size={px(24)} />
                </div>
            </div>
        }
    }
}

#[component]
pub(crate) struct SettingsValueRow<'a> {
    label: &'a str,
    value: &'a str,
    selected: bool,
}

impl RenderOnce for SettingsValueRow<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let background = row_background(self.selected);

        rsx! {
            <div class="w-full h-16 relative">
                <div class="absolute left-5 top-0 w-[440px] h-16 rounded-md bg-{background}" />

                <div class="absolute left-7 top-0 w-[290px] h-16 flex items-center">
                    <text class="text-xl no-wrap max-lines-1 text-ellipsis">
                        {self.label}
                    </text>
                </div>

                <div class="absolute right-7 top-0 w-[130px] h-16 flex items-center justify-end">
                    <text class="text-xl no-wrap max-lines-1 text-ellipsis">
                        {self.value}
                    </text>
                </div>
            </div>
        }
    }
}

#[component]
pub(crate) struct SettingsToggleRow<'a> {
    label: &'a str,
    enabled: bool,
    selected: bool,
}

impl RenderOnce for SettingsToggleRow<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let background = row_background(self.selected);

        let track_background = if self.enabled {
            Color::BLACK
        } else {
            Color::WHITE
        };

        let knob_background = if self.enabled {
            Color::WHITE
        } else {
            Color::BLACK
        };

        let knob_left = if self.enabled { px(19) } else { px(3) };

        rsx! {
            <div class="w-full h-16 relative">
                <div class="absolute left-5 top-0 w-[440px] h-16 rounded-md bg-{background}" />

                <div class="absolute left-7 top-0 w-[350px] h-16 flex items-center">
                    <text class="text-xl no-wrap max-lines-1 text-ellipsis">
                        {self.label}
                    </text>
                </div>

                <div class="absolute right-7 top-5 w-10 h-6 rounded-md border-px border-black bg-{track_background}">
                    <div class="absolute left-{knob_left} top-[3px] w-4 h-4 rounded-md bg-{knob_background}" />
                </div>
            </div>
        }
    }
}
