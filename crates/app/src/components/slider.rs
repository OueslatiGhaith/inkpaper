use inkpaper_ui::prelude::*;

const TRACK_PART: u64 = 2;

/// Lifecycle phase of a pointer scrub along a [`Slider`] track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SliderPhase {
    Start,
    Move,
    End,
}

/// Dispatched to a [`Slider`] track when the user presses, drags, or releases
/// along it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SliderScrubEvent {
    value: u8,
    phase: SliderPhase,
}

impl SliderScrubEvent {
    pub(crate) const fn new(value: u8, phase: SliderPhase) -> Self {
        Self { value, phase }
    }

    pub(crate) const fn value(self) -> u8 {
        self.value
    }

    pub(crate) const fn phase(self) -> SliderPhase {
        self.phase
    }
}

/// Whether `id` belongs to a [`Slider`] track rather than an overlay absorber.
pub(crate) const fn is_track_id(id: ElementId) -> bool {
    matches!(id, ElementId::NamedValue(_, TRACK_PART))
}

/// Maps a touch x coordinate to a slider value from 0 to 100 using the track's
/// laid-out screen bounds.
pub(crate) fn value_in_bounds(x: Pixels, bounds: Rect) -> u8 {
    let width = bounds.width().get().max(1);
    let relative = (x.get() - bounds.x().get()).clamp(0, width);

    ((relative * 100 + width / 2) / width) as u8
}

/// Slider row: `-` and `+` buttons around a track with a knob.
#[component]
pub(crate) struct Slider {
    id: &'static str,
    /// Knob position from 0 to 100.
    value: u8,
    on_decrease: Option<Listener<ActivateEvent>>,
    on_increase: Option<Listener<ActivateEvent>>,
    on_scrub: Option<Listener<SliderScrubEvent>>,
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

                <div
                    id={(self.id, TRACK_PART)}
                    on:SliderScrubEvent={self.on_scrub}
                    class="relative w-[296px] h-14"
                >
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
            <div
                id={self.id}
                on:activate={self.on_activate}
                class="w-14 h-14 flex items-center justify-center"
            >
                <text class="text-xl text-center">
                    {self.label}
                </text>
            </div>
        }
    }
}
