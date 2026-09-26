use inkpaper_ui::prelude::*;

use crate::components::icon::{Icon, IconKind, IconProps};

#[component]
pub(crate) struct TransferModeRow<'a> {
    id: &'static str,
    title: &'a str,
    subtitle: &'a str,
    icon: IconKind,
    on_activate: Listener<ActivateEvent>,
}

impl RenderOnce for TransferModeRow<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        rsx! {
            <div class="w-full h-15 relative">
                <div
                    id={self.id}
                    on:activate={self.on_activate}
                    class="absolute left-5 top-0 w-[440px] h-15 rounded-md"
                >
                    <div class="absolute left-2 top-0 h-15 flex items-center">
                        <Icon kind={self.icon} size={px(32)} />
                    </div>

                    <div class="absolute left-12 top-0 w-[384px] h-15 flex flex-col justify-center">
                        <text class="font-bold text-xl no-wrap max-lines-1 text-ellipsis">
                            {self.title}
                        </text>

                        <text class="text-base wrap max-lines-2 text-ellipsis">
                            {self.subtitle}
                        </text>
                    </div>
                </div>
            </div>
        }
    }
}
