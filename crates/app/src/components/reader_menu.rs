use alloc::format;

use inkpaper_ui::prelude::*;

use crate::{
    components::{
        drawer_handle::{DrawerHandle, DrawerHandleProps},
        settings_row::{ListRow, ListRowProps},
        slider::{self, Slider, SliderProps},
    },
    reader::{ReaderMenuTab, font_size_slider_value},
};

const CASE_SENSITIVE: SvgSource = include_svg!("assets/icons/lucide/case-sensitive.svg");
const ELLIPSIS: SvgSource = include_svg!("assets/icons/lucide/ellipsis.svg");

// screen geometry: the sheet starts 429 px above the bottom, the content 32 px
// below it, and the font slider after a 24 px label and a 4 px gap
const CONTENT_LEFT: i32 = 32;
const FONT_SLIDER_TOP: i32 = 371 + 32 + 24 + 4;

/// Whether `point` lands on the font size slider's track.
pub(crate) fn font_slider_contains(point: Point) -> bool {
    slider::track_contains(point, CONTENT_LEFT, FONT_SLIDER_TOP)
}

/// The font slider value at screen position `x`.
pub(crate) fn font_slider_value_at(x: i32) -> u8 {
    slider::value_at(x, CONTENT_LEFT)
}

/// Reader drawer: a bottom sheet over the page with a handle, a content pane and
/// an icon tab bar. Tabs are added as their features exist.
#[component]
pub(crate) struct ReaderMenu {
    tab: ReaderMenuTab,
    font_size: u16,
    on_close: Listener<ActivateEvent>,
    on_font_tab: Listener<ActivateEvent>,
    on_more_tab: Listener<ActivateEvent>,
    on_decrease_font_size: Listener<ActivateEvent>,
    on_increase_font_size: Listener<ActivateEvent>,
    on_select_chapter: Listener<ActivateEvent>,
}

impl RenderOnce for ReaderMenu {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let font_size_label = format!("Font Size  {}", self.font_size);

        let font_size_value = font_size_slider_value(self.font_size);

        rsx! {
            <div class="absolute left-0 top-0 w-[480px] h-[800px]">
                // taps above the sheet close the drawer
                <div
                    id="reader-menu-dismiss"
                    on:activate={self.on_close}
                    class="absolute left-0 top-0 w-full h-[371px]"
                />

                // 429 px sheet with a 3 px top rule
                <div class="absolute left-0 top-[371px] w-[480px] h-[429px] bg-white">
                    <div class="absolute left-0 top-0 w-full h-[3px] bg-black" />

                    <div
                        id="reader-menu-handle"
                        on:activate={self.on_close}
                        class="absolute left-[188px] top-0 w-[104px] h-[29px]"
                    >
                        <div class="absolute left-4 top-[13px]">
                            <DrawerHandle />
                        </div>
                    </div>

                    {#if self.tab == ReaderMenuTab::Font}
                        // content pane, inset like the control center's sliders
                        <div class="absolute left-8 top-8 w-[416px] flex flex-col">
                            <div class="w-full h-6 flex items-center">
                                <text class="text-base no-wrap">
                                    {font_size_label}
                                </text>
                            </div>

                            <div class="h-1" />

                            <Slider
                                id="reader-font-size"
                                value={font_size_value}
                                on_decrease={Some(self.on_decrease_font_size)}
                                on_increase={Some(self.on_increase_font_size)}
                            />
                        </div>
                    {:else}
                        <div class="absolute left-0 top-8 w-[480px] flex flex-col">
                            <ListRow
                                id={("reader-menu-select-chapter", 0)}
                                label="Select Chapter"
                                depth={0}
                                selected={false}
                                chevron={true}
                                on_activate={Some(self.on_select_chapter)}
                            />
                        </div>
                    {/if}

                    // 66 px tab bar under a 1 px rule; the active tab is inverted
                    <div class="absolute left-0 bottom-0 w-full h-[66px]">
                        <div class="absolute left-0 top-0 w-full h-px bg-black" />

                        <MenuTab
                            id="reader-menu-font-tab"
                            icon={CASE_SENSITIVE}
                            icon_size={px(32)}
                            left={px(4)}
                            active={self.tab == ReaderMenuTab::Font}
                            on_activate={self.on_font_tab}
                        />

                        <MenuTab
                            id="reader-menu-more-tab"
                            icon={ELLIPSIS}
                            icon_size={px(24)}
                            left={px(244)}
                            active={self.tab == ReaderMenuTab::More}
                            on_activate={self.on_more_tab}
                        />
                    </div>
                </div>
            </div>
        }
    }
}

#[component]
struct MenuTab {
    id: &'static str,
    icon: SvgSource,
    icon_size: Pixels,
    left: Pixels,
    active: bool,
    on_activate: Listener<ActivateEvent>,
}

impl RenderOnce for MenuTab {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let (background, foreground) = if self.active {
            (Color::BLACK, Color::WHITE)
        } else {
            (Color::WHITE, Color::BLACK)
        };

        rsx! {
            <div
                id={self.id}
                on:activate={self.on_activate}
                class="absolute left-{self.left} top-2 w-[232px] h-[46px] rounded-sm bg-{background} flex items-center justify-center"
            >
                {
                    svg(self.icon)
                        .size(Size::new(self.icon_size, self.icon_size))
                        .text_color(foreground)
                }
            </div>
        }
    }
}
