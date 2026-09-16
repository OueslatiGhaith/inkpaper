use inkpaper_ui::prelude::*;

use crate::components::icon::{Icon, IconKind, IconProps};

#[component]
pub(crate) struct CurrentBookCard<'a> {
    title: &'a str,
    subtitle: &'a str,
    on_activate: Listener<ActivateEvent>,
}

impl RenderOnce for CurrentBookCard<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        rsx! {
            <div
                id="home-current-book"
                on:activate={self.on_activate}
                class="w-full h-full relative rounded-md focus:bg-[#aaaaaa]"
            >
                <div class="absolute left-2 top-2 w-[150px] h-[226px] bg-white border-px border-black">
                    <div class="absolute left-0 top-[75px] w-[150px] h-[150px] bg-black" />

                    <div class="absolute left-6 top-6 w-8 h-8">
                        <Icon
                            kind={IconKind::BookOpen}
                            size={px(32)}
                        />
                    </div>
                </div>

                <div class="absolute left-[174px] top-0 w-[258px] h-[242px] flex flex-col justify-center gap-2">
                    <text class="font-bold text-2xl wrap max-lines-3 text-clip">
                        {self.title}
                    </text>

                    <text class="text-xl no-wrap max-lines-1 text-ellipsis">
                        {self.subtitle}
                    </text>

                    <text class="font-bold text-xl no-wrap max-lines-1 text-clip">
                        {"Continue reading"}
                    </text>
                </div>
            </div>
        }
    }
}
