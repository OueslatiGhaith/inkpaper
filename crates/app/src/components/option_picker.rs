use alloc::{string::String, vec::Vec};

use inkpaper_ui::prelude::*;

const SCREEN_HEIGHT: i32 = 800;
const ROW_HEIGHT: i32 = 56;
// the card's border and inner padding, above and below the rows
const CARD_INSET: i32 = 6;

#[component]
pub(crate) struct OptionPicker<'a> {
    title: &'a str,
    options: Vec<String>,
    selected: Option<usize>,
    on_option: Listener<ActivateEvent>,
    on_dismiss: Listener<ActivateEvent>,
}

impl RenderOnce for OptionPicker<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let rows = self.options.len() as i32 + 1;
        let height = rows * ROW_HEIGHT + CARD_INSET * 2;
        let top = px((SCREEN_HEIGHT - height) / 2);
        let height = px(height);

        let selected = self.selected;
        let on_option = self.on_option;

        let options = self
            .options
            .into_iter()
            .enumerate()
            .map(move |(index, label)| {
                OptionRow::from(OptionRowProps {
                    index,
                    label,
                    selected: selected == Some(index),
                    on_option,
                })
            });
        let options = div().w_full().flex().flex_col().children(options);

        rsx! {
            <div class="absolute left-0 top-0 w-[480px] h-[800px]">
                <div
                    id="option-picker-dismiss"
                    on:activate={self.on_dismiss}
                    class="absolute left-0 top-0 w-[480px] h-[800px]"
                />

                <div class="absolute left-5 top-{top} w-[440px] h-{height} rounded-md border-2 border-black bg-white">
                    <div class="absolute left-0 top-1 w-[436px] flex flex-col">
                        <div class="w-full h-14 relative">
                            <div class="absolute left-4 top-0 w-[404px] h-14 flex items-center">
                                <text class="text-xl font-bold no-wrap max-lines-1 text-ellipsis">
                                    {self.title}
                                </text>
                            </div>
                        </div>

                        {options}
                    </div>
                </div>
            </div>
        }
    }
}

#[component]
struct OptionRow {
    index: usize,
    label: String,
    selected: bool,
    on_option: Listener<ActivateEvent>,
}

impl RenderOnce for OptionRow {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let background = if self.selected {
            Color::BLACK
        } else {
            Color::WHITE
        };

        rsx! {
            <div
                id={("option-picker-option", self.index)}
                on:activate={self.on_option}
                class="w-full h-14 relative"
            >
                <div class="absolute left-2 top-1 w-[420px] h-12 rounded-sm bg-{background}" />

                <div class="absolute left-4 top-0 w-[404px] h-14 flex items-center">
                    {#if self.selected}
                        <text class="text-xl text-white no-wrap max-lines-1 text-ellipsis">
                            {self.label}
                        </text>
                    {:else}
                        <text class="text-xl no-wrap max-lines-1 text-ellipsis">
                            {self.label}
                        </text>
                    {/if}
                </div>
            </div>
        }
    }
}
