use inkpaper_ui::prelude::*;

use crate::components::icon::{Icon, IconKind, IconProps};

#[component]
pub(crate) struct RecentBookCard<'a> {
    title: &'a str,
    author: &'a str,
    progress: &'a str,
    selected: bool,
}

impl RenderOnce for RecentBookCard<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let background = if self.selected {
            Color::rgb(170, 170, 170)
        } else {
            Color::WHITE
        };

        let filled_width = (u32::from(parse_percent(self.progress)) * 258 / 100).saturating_sub(2);
        let progress_fill_width = px(filled_width as i32);

        rsx! {
            <div class="w-full h-full relative rounded-md bg-{background}">
                <div class="absolute left-[8px] top-[8px] w-[150px] h-[226px] bg-white border-px border-black">
                    <div class="absolute left-0 top-[75px] w-[150px] h-[150px] bg-black" />

                    <div class="absolute left-[24px] top-[24px] w-[32px] h-[32px]">
                        <Icon kind={IconKind::BookOpen} />
                    </div>
                </div>

                <div class="absolute left-[174px] top-0 w-[258px] h-[242px] flex flex-col justify-center">
                    <text class="text-[12px] leading-[16px] wrap max-lines-3 text-clip">{self.title}</text>

                    <div class="relative w-full h-[18px]">
                        <div class="absolute left-0 top-[6px] w-full h-[12px]">
                            <text class="text-[10px] leading-[12px] wrap max-lines-1 text-ellipsis">{self.author}</text>
                        </div>
                    </div>

                    <div class="relative w-full h-[24px]">
                        <div class="absolute left-0 top-[8px] w-full h-[12px]">
                            <text class="text-[10px] leading-[12px] no-wrap max-lines-1 text-clip">{self.progress}</text>
                        </div>

                        <div class="absolute left-0 top-[22px] w-[258px] h-[4px] border-px border-black">
                            <div class="absolute left-[1px] top-[1px] w-{progress_fill_width} h-[2px] bg-black" />
                        </div>
                    </div>
                </div>
            </div>
        }
    }
}

fn parse_percent(value: &str) -> u16 {
    let mut percent = 0u16;

    for byte in value.bytes() {
        if byte == b'%' {
            break;
        }

        if !byte.is_ascii_digit() {
            return 0;
        }

        percent = percent
            .saturating_mul(10)
            .saturating_add(u16::from(byte - b'0'));
    }

    percent.min(100)
}
