use inkpaper_ui::prelude::*;

use crate::components::{
    header::{BackHeader, BackHeaderProps},
    recent_book_row::{RecentBookRow, RecentBookRowProps},
};

#[component]
pub(crate) struct RecentBooksScreen;

impl RenderOnce for RecentBooksScreen {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        rsx! {
            <div class="w-[480px] h-[800px] relative bg-white text-black">
                <div class="absolute left-0 top-[5px] w-[480px] h-[77px]">
                    <BackHeader
                        title="Recent Books"
                        battery="72%"
                        charging={false}
                    />
                </div>

                <div class="absolute left-0 top-[98px] w-[480px] flex flex-col">
                    <RecentBookRow
                        title="Dune"
                        author="Frank Herbert"
                        selected={true}
                    />

                    <RecentBookRow
                        title="Project Hail Mary"
                        author="Andy Weir"
                        selected={false}
                    />

                    <RecentBookRow
                        title="The Three-Body Problem"
                        author="Cixin Liu"
                        selected={false}
                    />

                    <RecentBookRow
                        title="The Left Hand of Darkness"
                        author="Ursula K. Le Guin"
                        selected={false}
                    />

                    <RecentBookRow
                        title="Neuromancer"
                        author="William Gibson"
                        selected={false}
                    />

                    <RecentBookRow
                        title="Children of Time"
                        author="Adrian Tchaikovsky"
                        selected={false}
                    />

                    <RecentBookRow
                        title="The Dispossessed"
                        author="Ursula K. Le Guin"
                        selected={false}
                    />

                    <RecentBookRow
                        title="Hyperion"
                        author="Dan Simmons"
                        selected={false}
                    />
                </div>
            </div>
        }
    }
}
