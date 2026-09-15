use inkpaper_ui::prelude::*;

use crate::components::icon::{Icon, IconKind, IconProps};

#[component]
pub(crate) struct TransferModeRow<'a> {
    title: &'a str,
    subtitle: &'a str,
    icon: IconKind,
    selected: bool,
}

impl RenderOnce for TransferModeRow<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let background = if self.selected {
            Color::rgb(170, 170, 170)
        } else {
            Color::WHITE
        };

        rsx! {
            <div class="w-full h-15 relative">
                <div class="absolute left-5 top-0 w-[440px] h-15 rounded-md bg-{background}" />

                <div class="absolute left-7 top-0 h-15 flex items-center">
                    <Icon kind={self.icon} size={px(32)} />
                </div>

                <div class="absolute left-17 top-0 w-[384px] h-15 flex flex-col justify-center">
                    <text class="font-bold text-xl no-wrap max-lines-1 text-ellipsis">
                        {self.title}
                    </text>

                    <text class="text-base wrap max-lines-2 text-ellipsis">
                        {self.subtitle}
                    </text>
                </div>
            </div>
        }
    }
}
