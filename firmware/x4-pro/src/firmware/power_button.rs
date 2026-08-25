use defmt::{debug, info};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embassy_time::{Duration, Timer};
use esp_hal::{
    gpio::{Event, Input, InputConfig, Pull, WakeupConfig},
    peripherals::{GPIO3, LPWR},
    rtc_cntl::sleep::{LowPower, RtcSleepConfig},
};

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
    debug!("waiting for power button release...");

    loop {
        input.wait_for_high().await;
        Timer::after(Duration::from_millis(DEBOUNCE_MS)).await;

        if input.is_high() {
            break;
        }
    }

    // GPIO3 needs a low-power wake path because deep sleep powers down the normal
    // high-performance GPIO peripheral.
    input
        .apply_wakeup_config(&WakeupConfig::default().with_low_power_path(true))
        .unwrap();
    input.listen(Event::LowLevel);

    let mut low_power = LowPower::new(lpwr);
    info!("entering deep sleep");

    // does not return
    // pressing POWER pulls GPIO3 LOW. The deep-sleep wake resets the S3 and firmware
    // starts again from `main()`
    low_power.sleep_deep(RtcSleepConfig::deep());
}
