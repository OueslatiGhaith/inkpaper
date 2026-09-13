use inkpaper_ui::prelude::*;

use crate::components::icon::{Icon, IconKind};

pub(crate) struct RecentBookCardProps<'a> {
    pub title: &'a str,
    pub author: &'a str,
    pub progress: &'a str,
}

pub(crate) struct RecentBookCard<'a> {
    title: &'a str,
    author: &'a str,
    progress: &'a str,
}

impl<'a> From<RecentBookCardProps<'a>> for RecentBookCard<'a> {
    fn from(value: RecentBookCardProps<'a>) -> Self {
        Self {
            title: value.title,
            author: value.author,
            progress: value.progress,
        }
    }
}

impl RenderOnce for RecentBookCard<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let book_icon = Icon::new(IconKind::BookOpen);

        let progress_fill = rsx! {
            <div class="w-[173px] h-[2px] ml-[1px] mt-[1px] bg-black" />
        };

        rsx! {
            <div class="w-full h-full flex items-center px-[8px] gap-[16px]">
                <div class="w-[150px] h-[226px] relative border-px border-black">
                    <div class="absolute left-[24px] top-[24px]">
                        {book_icon}
                    </div>
                </div>

                <div class="flex-1 h-[226px] flex flex-col justify-center">
                    <text class="text-[12px] leading-[16px]">{self.title}</text>
                    <text class="text-[8px] leading-[12px]">{self.author}</text>
                    <div class="mt-[8px] w-full">
                        <text class="text-[10px] leading-[12px]">{self.progress}</text>
                    </div>
                    <div class="mt-[2px] w-full h-[4px] border-px border-black">
                        {progress_fill}
                    </div>
                </div>
            </div>
        }
    }
}
