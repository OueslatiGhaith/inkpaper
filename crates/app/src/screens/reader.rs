use inkpaper_ui::prelude::*;

use crate::components::header::{BackHeader, BackHeaderProps};

#[component]
pub(crate) struct ReaderScreen<'a> {
    title: &'a str,
    creator: &'a str,
    status: &'a str,
    detail: &'a str,
    path: &'a str,
    on_back: Listener<ActivateEvent>,
}

impl RenderOnce for ReaderScreen<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        rsx! {
            <div class="w-[480px] h-[800px] relative bg-white text-black">
                <div class="absolute left-0 top-[5px] w-[480px] h-[77px]">
                    <BackHeader
                        title={self.title}
                        battery="72%"
                        charging={false}
                        on_back={self.on_back}
                    />
                </div>

                <div class="absolute left-5 top-[220px] w-[440px] flex flex-col items-center gap-2.5">
                    <text class="font-bold text-2xl text-center no-wrap max-lines-1 text-ellipsis">
                        {self.title}
                    </text>

                    <text class="text-xl text-center no-wrap max-lines-1 text-ellipsis">
                        {self.creator}
                    </text>

                    <text class="text-xl text-center">
                        {self.status}
                    </text>

                    <div class="mt-2.5 w-[420px]">
                        <text class="text-base text-center no-wrap max-lines-1 text-ellipsis">
                            {self.detail}
                        </text>
                    </div>

                    <div class="w-[420px]">
                        <text class="text-base text-center no-wrap max-lines-1 text-ellipsis">
                            {self.path}
                        </text>
                    </div>
                </div>
            </div>
        }
    }
}
