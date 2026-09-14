use inkpaper_ui::prelude::*;

use crate::components::{
    header::{HomeHeader, HomeHeaderProps},
    home_menu::{HomeMenu, HomeMenuProps},
    recent_book::{RecentBookCard, RecentBookCardProps},
};

#[component]
pub(crate) struct HomeScreen;

impl RenderOnce for HomeScreen {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        rsx! {
            <div class="w-[480px] h-[800px] relative bg-white text-black">
                <div class="absolute left-0 top-[5px] w-[480px] h-14">
                    <HomeHeader battery="72%" />
                </div>

                <div class="absolute left-5 top-[56px] w-[440px] h-[242px]">
                    <RecentBookCard
                        title="Book 1"
                        author="Author 1"
                        progress="68%"
                        selected={true}
                    />
                </div>

                <div class="absolute left-5 top-[314px] w-[440px]">
                    <HomeMenu />
                </div>
            </div>
        }
    }
}
