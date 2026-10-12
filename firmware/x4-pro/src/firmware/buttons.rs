use embassy_futures::select::select;
use embassy_time::{Duration, Timer};
use esp_hal::gpio::Input;

use crate::firmware::input::{Button, ButtonEdge, ButtonEvent, INPUT_EVENTS, InputEvent};

const SCAN_INTERVAL_MS: u64 = 10;

/// a changed electrical level must be observed for 3 consecutive scans before it
/// becomes a logical button edge
const DEBOUNCE_SAMPLES: u8 = 3;

struct DebouncedButton<'d> {
    input: Input<'d>,
    stable_pressed: bool,
    candidate_pressed: bool,
    candidate_samples: u8,
}

impl<'d> DebouncedButton<'d> {
    fn new(input: Input<'d>) -> Self {
        let pressed = input.is_low();

        Self {
            input,
            stable_pressed: pressed,
            candidate_pressed: pressed,
            candidate_samples: 0,
        }
    }

    fn sample(&mut self) -> Option<ButtonEdge> {
        let pressed = self.input.is_low();

        // we're back at the already accepted state.
        // any transient candidate was just switch bounce
        if pressed == self.stable_pressed {
            self.candidate_pressed = pressed;
            self.candidate_samples = 0;
            return None;
        }

        // the raw level changed to a new candidate
        if pressed != self.candidate_pressed {
            self.candidate_pressed = pressed;
            self.candidate_samples = 1;
            return None;
        }

        self.candidate_samples = self.candidate_samples.saturating_add(1);
        if self.candidate_samples < DEBOUNCE_SAMPLES {
            return None;
        }

        // the candidate has remained stable long enough
        self.stable_pressed = self.candidate_pressed;
        self.candidate_samples = 0;

        Some(if self.stable_pressed {
            ButtonEdge::Pressed
        } else {
            ButtonEdge::Released
        })
    }

    /// the last sample matched the accepted state, so no edge is in progress
    fn is_settled(&self) -> bool {
        self.candidate_samples == 0
    }

    /// waits until the raw level differs from the accepted state.
    ///
    /// waiting on a level rather than an edge returns at once if the pin already
    /// changed since the last sample, so no press is lost between scan and wait
    async fn wait_for_change(&mut self) {
        if self.stable_pressed {
            self.input.wait_for_high().await;
        } else {
            self.input.wait_for_low().await;
        }
    }
}

pub struct Buttons<'d> {
    left: DebouncedButton<'d>,
    right: DebouncedButton<'d>,
}

impl<'d> Buttons<'d> {
    pub fn new(left: Input<'d>, right: Input<'d>) -> Self {
        Self {
            left: DebouncedButton::new(left),
            right: DebouncedButton::new(right),
        }
    }

    fn sample(&mut self) -> [Option<ButtonEvent>; 2] {
        [
            self.left
                .sample()
                .map(|edge| ButtonEvent::new(Button::Left, edge)),
            self.right
                .sample()
                .map(|edge| ButtonEvent::new(Button::Right, edge)),
        ]
    }

    fn is_settled(&self) -> bool {
        self.left.is_settled() && self.right.is_settled()
    }

    async fn wait_for_change(&mut self) {
        select(self.left.wait_for_change(), self.right.wait_for_change()).await;
    }
}

#[embassy_executor::task]
pub async fn button_task(mut buttons: Buttons<'static>) {
    loop {
        let events = buttons.sample();
        for event in events.into_iter().flatten() {
            // don't silently discard physical input
            // if the display task falls behind, backpressure here is preferrable
            // to losing the button edge
            INPUT_EVENTS.send(InputEvent::Button(event)).await;
        }

        // only scan on a timer while a button is changing; otherwise sleep until
        // one of the pins leaves its accepted level
        if buttons.is_settled() {
            buttons.wait_for_change().await;
        } else {
            Timer::after(Duration::from_millis(SCAN_INTERVAL_MS)).await;
        }
    }
}
