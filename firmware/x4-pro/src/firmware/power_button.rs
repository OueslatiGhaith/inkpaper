use defmt::{debug, info};
use embassy_futures::select::{Either, select};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embassy_time::{Duration, Timer};
use esp_hal::{
    gpio::{Event, Input, InputConfig, Pull, WakeupConfig},
    peripherals::{GPIO3, LPWR},
    rtc_cntl::sleep::{LowPower, RtcSleepConfig},
};

use crate::firmware::input::{INPUT_EVENTS, InputEvent, PowerButtonEvent};

const DEBOUNCE_MS: u64 = 30;
const LONG_PRESS_MS: u64 = 1_000;
const LONG_PRESS_AFTER_DEBOUNCE_MS: u64 = LONG_PRESS_MS - DEBOUNCE_MS;

pub static ENTER_DEEP_SLEEP: Signal<CriticalSectionRawMutex, ()> = Signal::new();

#[embassy_executor::task]
pub async fn power_button_task(mut pin: GPIO3<'static>, lpwr: LPWR<'static>) {
    let mut input = Input::new(pin.reborrow(), InputConfig::default().with_pull(Pull::Up));

    // main performs the expensive/ordered shutdown first:
    // - frontlight off
    // - flush app state
    // - storage shutdown
    // - EPD deep sleep
    // - peripheral rails off
    // it signals us only when we're allowed to enter SoC deep sleep, whether the
    // shutdown came from a long press or from auto-sleep
    select(report_presses(&mut input), ENTER_DEEP_SLEEP.wait()).await;
    debug!("waiting for power button release...");

    wait_for_release(&mut input).await;

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

/// Reports debounced short and long presses until the task stops listening.
async fn report_presses(input: &mut Input<'_>) -> ! {
    // if deep sleep woke us because POWER was held LOW, the new firmware instance
    // starts while GPIO3 may still be LOW.
    // waiting specifically for a falling edge means the wake-up press cannot immediately
    // be interpreted as a new "go to sleep" press
    loop {
        input.wait_for_falling_edge().await;
        Timer::after(Duration::from_millis(DEBOUNCE_MS)).await;

        if !input.is_low() {
            continue;
        }

        // a normal click must not put the device to sleep, After the debounce period, race
        // the rest of the long press interval against release.
        let event = match select(
            Timer::after(Duration::from_millis(LONG_PRESS_AFTER_DEBOUNCE_MS)),
            input.wait_for_high(),
        )
        .await
        {
            // a release may land at nearly the same instant as the timer
            Either::First(()) if input.is_low() => {
                info!("power long press detected");

                PowerButtonEvent::LongPress
            }
            _ => {
                debug!("power short press detected");

                PowerButtonEvent::ShortPress
            }
        };

        INPUT_EVENTS.send(InputEvent::Power(event)).await;
    }
}

async fn wait_for_release(input: &mut Input<'_>) {
    loop {
        input.wait_for_high().await;
        Timer::after(Duration::from_millis(DEBOUNCE_MS)).await;

        if input.is_high() {
            break;
        }
    }
}
