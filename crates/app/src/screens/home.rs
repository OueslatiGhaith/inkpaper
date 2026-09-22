use inkpaper_ui::prelude::*;

use crate::{
    BatteryStatus, ReadingHistoryEntry,
    components::{
        current_book_card::{CurrentBookCard, CurrentBookCardProps},
        header::{HomeHeader, HomeHeaderProps},
        home_menu::{HomeMenu, HomeMenuProps},
    },
};

#[component]
pub(crate) struct HomeScreen<'a> {
    current_book: Option<&'a ReadingHistoryEntry>,
    battery: Option<BatteryStatus>,
    on_current_book: Listener<ActivateEvent>,
    on_browse_files: Listener<ActivateEvent>,
    on_recent_books: Listener<ActivateEvent>,
    on_file_transfer: Listener<ActivateEvent>,
    on_settings: Listener<ActivateEvent>,
}

impl RenderOnce for HomeScreen<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        rsx! {
            <div class="w-[480px] h-[800px] relative bg-white text-black">
                <div class="absolute left-0 top-[5px] w-[480px] h-14">
                    <HomeHeader battery={self.battery} />
                </div>

                <div class="absolute left-5 top-14 w-[440px] h-[242px]">
                    {#if let Some(book) = self.current_book}
                        <CurrentBookCard
                            title={book.display_title()}
                            subtitle={book.display_subtitle()}
                            progress={book.progress()}
                            on_activate={self.on_current_book}
                        />
                    {:else}
                        <div class="w-full h-full flex flex-col items-center justify-center gap-2">
                            <text class="font-bold text-2xl">
                                {"No book in progress"}
                            </text>

                            <text class="text-xl">
                                {"Open an EPUB to start reading"}
                            </text>
                        </div>
                    {/if}
                </div>

                <div class="absolute left-5 top-[314px] w-[440px]">
                    <HomeMenu
                        on_browse_files={self.on_browse_files}
                        on_recent_books={self.on_recent_books}
                        on_file_transfer={self.on_file_transfer}
                        on_settings={self.on_settings}
                    />
                </div>
            </div>
        }
    }
}
