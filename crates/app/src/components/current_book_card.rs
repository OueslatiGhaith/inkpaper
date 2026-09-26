use alloc::format;

use inkpaper_ui::prelude::*;

use crate::{
    BookProgress,
    components::icon::{Icon, IconKind, IconProps},
};

#[component]
pub(crate) struct CurrentBookCard<'a> {
    title: &'a str,
    subtitle: &'a str,
    progress: BookProgress,
    on_activate: Listener<ActivateEvent>,
}

impl RenderOnce for CurrentBookCard<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let progress_label = format!("{}%", self.progress.percent());

        // 258 px outer width with one-pixel borders leaves a 256 px interior.
        let filled_width = u32::from(self.progress.basis_points()) * 256 / 10_000;
        let progress_fill_width = px(filled_width as i32);

        rsx! {
            <div
                id="home-current-book"
                on:activate={self.on_activate}
                class="w-full h-full relative rounded-md"
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

                    <div class=" w-full flex flex-col gap-1">
                        <text class="font-bold text-xl no-wrap max-lines-1 text-clip">
                            {progress_label}
                        </text>

                        <div class=" relative w-full h-1 border-px border-black">
                            <div class="absolute left-px top-px w-{progress_fill_width} h-[2px] bg-black" />
                        </div>
                    </div>
                </div>
            </div>
        }
    }
}
