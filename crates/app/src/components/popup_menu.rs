use alloc::vec::Vec;

use inkpaper_ui::prelude::*;

const ITEM_HEIGHT: i32 = 56;
// the card's border and inner padding, above and below the items
const CARD_INSET: i32 = 6;
// keeps the card clear of the finger that opened it
const FINGER_GAP: i32 = 28;
// below the header, and above the bottom edge
const SCREEN_TOP: i32 = 90;
const SCREEN_BOTTOM: i32 = 790;

/// An item of a [`PopupMenu`]: a label, with an optional value on the right.
#[derive(Debug, Clone, Copy)]
pub(crate) struct MenuItem<'a> {
    pub(crate) label: &'a str,
    pub(crate) value: &'a str,
}

/// A card of items floating over the screen, opened next to a point such as
/// a long press. Tapping outside it dismisses it. Every item shares
/// `on_item`; the item's index is its element id.
#[component]
pub(crate) struct PopupMenu<'a> {
    /// how far down the screen the menu was opened
    anchor_y: i32,
    items: Vec<MenuItem<'a>>,
    on_item: Listener<ActivateEvent>,
    on_dismiss: Listener<ActivateEvent>,
}

impl RenderOnce for PopupMenu<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let height = self.items.len() as i32 * ITEM_HEIGHT + CARD_INSET * 2;

        // below the finger when it fits, otherwise above it
        let below = self.anchor_y + FINGER_GAP;
        let top = if below + height <= SCREEN_BOTTOM {
            below
        } else {
            (self.anchor_y - FINGER_GAP - height).max(SCREEN_TOP)
        };

        let (top, height) = (px(top), px(height));
        let on_item = self.on_item;

        let rows = self
            .items
            .into_iter()
            .enumerate()
            .map(move |(index, item)| {
                PopupMenuRow::from(PopupMenuRowProps {
                    index,
                    item,
                    on_item,
                })
            });
        let items = div().w_full().flex().flex_col().children(rows);

        rsx! {
            <div class="absolute left-0 top-0 w-[480px] h-[800px]">
                <div
                    id="popup-menu-dismiss"
                    on:activate={self.on_dismiss}
                    class="absolute left-0 top-0 w-[480px] h-[800px]"
                />

                <div class="absolute left-5 top-{top} w-[440px] h-{height} rounded-md border-2 border-black bg-white">
                    <div class="absolute left-0 top-1 w-[436px]">
                        {items}
                    </div>
                </div>
            </div>
        }
    }
}

#[component]
struct PopupMenuRow<'a> {
    index: usize,
    item: MenuItem<'a>,
    on_item: Listener<ActivateEvent>,
}

impl RenderOnce for PopupMenuRow<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        rsx! {
            <div
                id={("popup-menu-item", self.index)}
                on:activate={self.on_item}
                class="w-full h-14 relative"
            >
                <div class="absolute left-4 top-0 w-[270px] h-14 flex items-center">
                    <text class="text-xl no-wrap max-lines-1 text-ellipsis">
                        {self.item.label}
                    </text>
                </div>

                <div class="absolute right-4 top-0 w-[120px] h-14 flex items-center justify-end">
                    <text class="text-xl no-wrap max-lines-1">
                        {self.item.value}
                    </text>
                </div>
            </div>
        }
    }
}
