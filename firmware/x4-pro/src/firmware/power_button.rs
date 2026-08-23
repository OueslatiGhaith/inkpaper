use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embassy_time::{Duration, Timer};
use esp_hal::{
    gpio::{Input, InputConfig, Pull, RtcPinWithResistors},
    peripherals::{GPIO3, LPWR},
    rtc_cntl::{
        Rtc,
        sleep::{Ext0WakeupSource, WakeupLevel},
    },
};
use esp_println::println;

use crate::firmware::input::{Button, ButtonEdge, ButtonEvent, INPUT_EVENTS, InputEvent};

const DEBOUNCE_MS: u64 = 30;

pub static ENTER_DEEP_SLEEP: Signal<CriticalSectionRawMutex, ()> = Signal::new();

#[embassy_executor::task]
pub async fn power_button_task(mut pin: GPIO3<'static>, lpwr: LPWR<'static>) {
    let mut input = Input::new(pin.reborrow(), InputConfig::default().with_pull(Pull::Up));

    // if deep sleep woke us because POWER was helo LOW, the new firmware instance
    // starts while GPIO3 may still be LOW.
    // waiting specifically for a falling edge means the wake-up press cannot immediately
    // be interpreted as a new "go to sleep" press
    loop {
        input.wait_for_falling_edge().await;
        Timer::after(Duration::from_millis(DEBOUNCE_MS)).await;

        if !input.is_low() {
            continue;
        }

        INPUT_EVENTS
            .send(InputEvent::Button(ButtonEvent::new(
                Button::Power,
                ButtonEdge::Pressed,
            )))
            .await;

        break;
    }

    // main performs the expensive/ordered shutdown first:
    // - EPD deep sleep
    // - peripheral rails off
    // it signals us only when we're allowed to ender SoC deep sleep
    ENTER_DEEP_SLEEP.wait().await;
    println!("waiting for power button release...");

    loop {
        input.wait_for_high().await;
        Timer::after(Duration::from_micros(DEBOUNCE_MS)).await;

        if input.is_high() {
            break;
        }
    }

    // EXT0 needs ownership of the RTC-capable GPIO, so release the normal digital
    // input dirver first
    drop(input);

    // GPIO3 has no guaranteed external pull-up, so configure the RTC-domain pull
    // explicitly before switching it to RTC wake
    pin.rtcio_pullup(true);
    pin.rtcio_pulldown(false);

    let wake = Ext0WakeupSource::new(pin, WakeupLevel::Low);
    let mut rtc = Rtc::new(lpwr);
    println!("entering deep sleep");

    // does not return. A LOW transition on GPIO3 resets/wakes the S3, starting
    // firmware again from `main()`
    rtc.sleep_deep(&[&wake]);
}
