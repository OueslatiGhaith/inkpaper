use inkpaper_ui::prelude::*;

use crate::components::icon::{Icon, IconKind, IconProps};

pub(crate) const RECENT_BOOK_ROW_HEIGHT: i32 = 64;

#[component]
pub(crate) struct RecentBookRow<'a> {
    id: usize,
    title: &'a str,
    subtitle: &'a str,
    on_activate: Listener<ActivateEvent>,
}

impl RenderOnce for RecentBookRow<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let height = px(RECENT_BOOK_ROW_HEIGHT);

        rsx! {
            <div
                id={("recent-book-row", self.id)}
                on:activate={self.on_activate}
                class="ml-5 w-[440px] h-{height} relative rounded-md"
            >
                <div class="absolute left-2 top-0 h-{height} flex items-center">
                    <Icon kind={IconKind::Book} size={px(28)} />
                </div>

                <div class="absolute left-11 top-0 w-[388px] h-{height} flex flex-col justify-center">
                    <text class="font-bold text-xl no-wrap max-lines-1 text-ellipsis">
                        {self.title}
                    </text>

                    <text class="text-base no-wrap max-lines-1 text-ellipsis">
                        {self.subtitle}
                    </text>
                </div>
            </div>
        }
    }
}
