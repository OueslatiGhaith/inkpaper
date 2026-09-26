use inkpaper_ui::prelude::*;

// the track area sits between the 56 px buttons and their 4 px gaps
const TRACK_OFFSET: i32 = 60;
const TRACK_WIDTH: i32 = 296;
const ROW_HEIGHT: i32 = 56;

/// Whether `point` lands on the track of a slider row whose top-left corner
/// is at (`row_left`, `row_top`) on screen.
pub(crate) fn track_contains(point: Point, row_left: i32, row_top: i32) -> bool {
    let x = point.x.get() - row_left - TRACK_OFFSET;
    let y = point.y.get() - row_top;

    (0..TRACK_WIDTH).contains(&x) && (0..ROW_HEIGHT).contains(&y)
}

/// Maps a touch x position to a slider value from 0 to 100, for a slider row
/// whose left edge is at `row_left` on screen.
pub(crate) fn value_at(x: i32, row_left: i32) -> u8 {
    let relative = (x - row_left - TRACK_OFFSET).clamp(0, TRACK_WIDTH);

    ((relative * 100 + TRACK_WIDTH / 2) / TRACK_WIDTH) as u8
}

/// Slider row: `-` and `+` buttons around a track with a knob.
///
/// The buttons only take taps when a listener is given. The control center hit-tests
/// its sliders itself.
#[component]
pub(crate) struct Slider {
    id: &'static str,
    /// Knob position from 0 to 100.
    value: u8,
    on_decrease: Option<Listener<ActivateEvent>>,
    on_increase: Option<Listener<ActivateEvent>>,
}

impl RenderOnce for Slider {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let value = i32::from(self.value.min(100));
        let fill_width = px(value * 280 / 100);
        let knob_left = px(8 + value * 266 / 100);

        rsx! {
            <div class="w-full h-14 flex items-center gap-1">
                <SliderButton
                    id={(self.id, 0)}
                    label="-"
                    on_activate={self.on_decrease}
                />

                <div class="relative w-[296px] h-14">
                    // FreeInk's normal slider has 8 px horizontal padding,
                    // a 4 px light-gray track, and a 14x22 black knob.
                    <div class="absolute left-2 top-[26px] w-[280px] h-1 bg-[#aaaaaa]" />

                    <div class="absolute left-2 top-[26px] w-{fill_width} h-1 bg-black" />

                    <div class="absolute left-{knob_left} top-[17px] w-[14px] h-[22px] bg-black" />
                </div>

                <SliderButton
                    id={(self.id, 1)}
                    label="+"
                    on_activate={self.on_increase}
                />
            </div>
        }
    }
}

#[component]
struct SliderButton {
    id: (&'static str, u8),
    label: &'static str,
    on_activate: Option<Listener<ActivateEvent>>,
}

impl RenderOnce for SliderButton {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        rsx! {
            <div class="w-14 h-14">
                {#if let Some(listener) = self.on_activate}
                    <div
                        id={self.id}
                        on:activate={listener}
                        class="w-full h-full flex items-center justify-center focus:bg-[#aaaaaa]"
                    >
                        <text class="text-xl text-center">
                            {self.label}
                        </text>
                    </div>
                {:else}
                    <div class="w-full h-full flex items-center justify-center">
                        <text class="text-xl text-center">
                            {self.label}
                        </text>
                    </div>
                {/if}
            </div>
        }
    }
}
