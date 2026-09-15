use inkpaper_ui::prelude::*;

use crate::components::icon::{Icon, IconKind, IconProps};

#[component]
pub(crate) struct RecentBookRow<'a> {
    title: &'a str,
    author: &'a str,
    selected: bool,
}

impl RenderOnce for RecentBookRow<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let background = if self.selected {
            Color::rgb(170, 170, 170)
        } else {
            Color::WHITE
        };

        rsx! {
            <div class="w-full h-16 relative">
                <div class="absolute left-5 top-0 w-[440px] h-16 rounded-md bg-{background}" />

                <div class="absolute left-7 top-0 h-16 flex items-center">
                    <Icon kind={IconKind::Book} size={px(28)} />
                </div>

                <div class="absolute left-16 top-0 w-[388px] h-16 flex flex-col justify-center">
                    <text class="font-bold text-xl no-wrap max-lines-1 text-ellipsis">
                        {self.title}
                    </text>

                    <text class="text-base no-wrap max-lines-1 text-ellipsis">
                        {self.author}
                    </text>
                </div>
            </div>
        }
    }
}
