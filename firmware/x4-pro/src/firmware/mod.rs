use defmt::{debug, error, info, warn};
use embassy_executor::Spawner;
use embassy_futures::select::{Either3, select3};
use embassy_time::{Delay as AsyncDelay, Duration, Timer};
use epd_bus::SpiEpdBus;
use esp_backtrace as _;
use esp_hal::{
    clock::CpuClock,
    gpio::{Input, InputConfig, Pull},
    i2c::master::{Config as I2cConfig, I2c},
    spi::{
        Mode,
        master::{Config as SpiConfig, Spi},
    },
    time::Rate,
    timer::timg::TimerGroup,
};
use inkpaper_app::{AppModel, BookSummary, InkPaperApp, Route, clock::ClockError, theme::Theme};
use inkpaper_ui::prelude::*;
use static_cell::StaticCell;
use xteink_display_probe::{Verdict, detect_x4_controller};

use crate::firmware::{
    battery::{BATTERY_UPDATES, BatteryReading, battery_task},
    buttons::{Buttons, button_task},
    display::X4Panel,
    framebuffer::FRAMEBUFFER_LEN,
    frontlight::{frontlight_off_and_wait, frontlight_task},
    input::{Button, ButtonEdge, ButtonEvent, INPUT_EVENTS, InputEvent, TouchEvent, TouchPosition},
    power::PowerRails,
    power_button::{ENTER_DEEP_SLEEP, power_button_task},
    presenter::{Presenter, UiRuntime},
    probe::ProbePins,
    rtc::{RTC_UPDATES, RtcState, rtc_task},
    sleep_pins::{hold_for_deep_sleep, release_display_reset_hold},
    touch::{TouchController, touch_task},
};

mod battery;
mod buttons;
mod display;
mod framebuffer;
mod frontlight;
mod i2c_bus;
mod input;
mod power;
mod power_button;
mod presenter;
mod probe;
mod rtc;
mod sleep_pins;
mod storage;
mod touch;

esp_bootloader_esp_idf::esp_app_desc!();

static FRAMEBUFFER: StaticCell<[u8; FRAMEBUFFER_LEN]> = StaticCell::new();
static UI_RUNTIME: StaticCell<UiRuntime> = StaticCell::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InputAction {
    Continue,
    Sleep,
}

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    rtt_target::rtt_init_defmt!();
    info!("InkPaper X4 Pro boot");

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // establish the board's safe rail state before doing anything else
    let (mut rails, sd_power) =
        PowerRails::new(peripherals.GPIO1, peripherals.GPIO2, peripherals.GPIO5);

    let timer_group = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timer_group.timer0, peripherals.FROM_CPU_INTR0);

    spawner.spawn(frontlight_task(peripherals.LEDC, peripherals.GPIO8, peripherals.GPIO9).unwrap());

    {
        let buttons = Buttons::new(
            Input::new(
                peripherals.GPIO0,
                InputConfig::default().with_pull(Pull::Up),
            ),
            Input::new(
                peripherals.GPIO7,
                InputConfig::default().with_pull(Pull::Up),
            ),
        );
        spawner.spawn(button_task(buttons).unwrap());
        spawner.spawn(power_button_task(peripherals.GPIO3, peripherals.LPWR).unwrap());
    }

    let mut delay = AsyncDelay;

    spawner.spawn(
        storage::storage_task(
            peripherals.SDHOST,
            peripherals.GPIO41,
            peripherals.GPIO42,
            peripherals.GPIO40,
            sd_power,
        )
        .unwrap(),
    );

    let storage_ready = storage::wait_ready().await;
    info!("storage ready: {}", storage_ready);
    if storage_ready {
        let listed = storage::list_root_and_wait().await;
        debug!("storage root listing: {}", listed);
    }

    info!("probing display controller...");

    let mut probe_io = ProbePins::new(
        peripherals.GPIO12,
        peripherals.GPIO11,
        peripherals.GPIO13,
        peripherals.GPIO18,
        peripherals.GPIO14,
    );

    // if this boot followed deep sleep, GPIO14 is still held HIGH
    // ProbePins has now configured the new active GPIO state to output HIGH, so it is
    // safe to release that previous RTC hold
    release_display_reset_hold();

    let detection = detect_x4_controller(&mut probe_io, &mut delay)
        .await
        .unwrap();

    info!(
        "display probe controller={} verdict={}",
        detection.controller, detection.verdict,
    );
    debug!(
        "display probe version={} flags={:#04x} mtp_read={}",
        detection.diagnostics.version.bytes(),
        detection.diagnostics.flags.raw(),
        detection.diagnostics.mtp.is_some(),
    );

    // an inconclusive result means something on the desplay bus responded, but not
    // consistently enough to identify it safely. Do not send an SSD1677 initialization
    // sequence to potential UC silicon
    if detection.verdict == Verdict::Inconclusive {
        error!("display probe inconclusive, refusing to drive panel");
        stay_alive().await;
    }

    // the probe has finished using the display pins
    let (sclk, mosi, cs, dc, reset) = probe_io.into_parts();
    let spi = Spi::new(
        peripherals.SPI2,
        SpiConfig::default()
            .with_frequency(Rate::from_mhz(5))
            .with_mode(Mode::_0),
    )
    .unwrap()
    .with_sck(sclk.into_peripheral_output())
    .with_mosi(mosi.into_peripheral_output())
    .into_async();

    let busy = Input::new(peripherals.GPIO6, InputConfig::default());
    let mut bus = SpiEpdBus::new(spi, cs, dc, reset, busy).unwrap();

    let mut panel = X4Panel::new(detection.controller);

    info!("initializing display...");
    panel.initialize(&mut bus, &mut delay).await.unwrap();
    info!("display initialized");

    let frame = FRAMEBUFFER.init_with(|| [0xff; FRAMEBUFFER_LEN]);
    let runtime = UI_RUNTIME.init_with(UiRuntime::default);
    runtime.set_global(Theme::EINK).unwrap();

    let model = demo_model();
    let app = runtime.create(move |_| InkPaperApp::new(model)).unwrap();
    let mut presenter = Presenter::default();

    debug!("building UI frame...");
    let update = presenter.render_initial(runtime, app, frame);
    let damage = update.physical_damage();
    defmt::debug!(
        "initial damage x={} y={} width={} height={}",
        damage.x,
        damage.y,
        damage.width,
        damage.height,
    );

    info!("presenting initial frame...");
    panel
        .present(&mut bus, &mut delay, frame, update)
        .await
        .unwrap();
    info!("initial display complete");

    frontlight::set(::frontlight::Setting::new(
        ::frontlight::Percent::new(25).unwrap(),
        ::frontlight::Percent::new(50).unwrap(),
    ))
    .await;

    let i2c = I2c::new(
        peripherals.I2C0,
        I2cConfig::default().with_frequency(Rate::from_khz(400)),
    )
    .unwrap()
    .with_sda(peripherals.GPIO39)
    .with_scl(peripherals.GPIO38)
    .into_async();
    let shared_i2c = i2c_bus::init(i2c);

    // touch power-up.
    // GPIO2 is active-low. Give the GT911 rail time to settle before
    // performing its address-select reset sequence.
    debug!("powering GT911...");
    rails.enable_touch();
    Timer::after(Duration::from_millis(50)).await;

    let mut touch = TouchController::new(
        i2c_bus::device(shared_i2c),
        peripherals.GPIO4,
        peripherals.GPIO10,
    );

    match touch.initialize(&mut delay).await {
        Ok(info) => {
            info!(
                "GT911 initialized address={:#04x} firmware={:#06x} resolution={}x{} vendor={:#04x}",
                touch.address(),
                info.firmware_version(),
                info.x_resolution(),
                info.y_resolution(),
                info.vendor_id(),
            );
            debug!("GT911 product={}", info.product_id());

            spawner.spawn(touch_task(touch).unwrap());
        }
        Err(error) => {
            warn!("GT911 initialization failed error={:?}", error);
            rails.disable_touch();
        }
    }

    spawner.spawn(battery_task(i2c_bus::device(shared_i2c)).unwrap());
    spawner.spawn(rtc_task(i2c_bus::device(shared_i2c)).unwrap());
    info!("shared I2C services started");

    loop {
        let mut action = InputAction::Continue;

        // main sleeps until either:
        // - physical user input arrives
        // - the battery service has a new reading
        // `BATTERY_UPDATES` is a signal, so dropping its pending wait when input
        // wins this select is safe and doesn't lose a stored reading
        match select3(
            INPUT_EVENTS.receive(),
            BATTERY_UPDATES.wait(),
            RTC_UPDATES.wait(),
        )
        .await
        {
            Either3::First(event) => {
                action = handle_input_event(runtime, app, event);
                // combine events accumulated while the e-ink panel was busy.
                while action == InputAction::Continue {
                    let Ok(event) = INPUT_EVENTS.try_receive() else {
                        break;
                    };
                    action = handle_input_event(runtime, app, event);
                }
            }
            Either3::Second(reading) => apply_battery_reading(runtime, app, reading),
            Either3::Third(state) => apply_rtc_state(runtime, app, state),
        }

        if action == InputAction::Sleep {
            info!("suspend: turning frontlight off");
            frontlight_off_and_wait().await;

            info!("suspend: shutting down storage");
            storage::shutdown_and_wait().await;
            info!("suspend: storage shutdown complete");

            info!("suspend: putting display controller to sleep");

            // ORDER MATTERS:
            // step 1:
            // tell the actual display controller to enter its own low-power state
            // while SPI, RESET, adn the board rails are all still operational
            panel.deep_sleep(&mut bus, &mut delay).await.unwrap();
            info!("suspend: display controller asleep");

            // step2:
            // the X4 PRO keeps the panel rail powered in deep sleep. Force RESET high
            // before latching the pin so a sleeping UC controller can't drift back into
            // an active state
            bus.reset_high().unwrap();

            // step3:
            // latch GPIO1, GPIO2, GPIO5, and GPIO14 while they are actively driven
            // to those known states.
            // the RTC pad-hold bits survive the ESP32-S3 deep-sleep interval and remain
            // set until the next boot deliberately releases them
            rails.prepare_for_deep_sleep();
            hold_for_deep_sleep();
            info!("suspend: board pins latched for deep sleep");

            // step5:
            // the power task owns GPIO3 and LPWR. It waits for the current button press
            // to be released, arms EXT0 LOW, then performs the final SoC deep-sleep
            // transition
            ENTER_DEEP_SLEEP.signal(());

            // `power_button_task()` will take the MCU into deep sleep.
            // nothing in this task should touch the hardware again
            stay_alive().await;
        }

        let Some(update) = presenter.render_pending(runtime, app, frame) else {
            continue;
        };

        let damage = update.physical_damage();
        debug!(
            "UI update refresh={:?} x={} y={} width={} height={}",
            update.refresh(),
            damage.x,
            damage.y,
            damage.width,
            damage.height,
        );

        panel
            .present(&mut bus, &mut delay, frame, update)
            .await
            .unwrap();
    }
}

fn demo_model() -> AppModel {
    let book = BookSummary::try_new("The Left Hand of Darkness", "Ursula K. Le Guin", 68)
        .expect("demo book metadata must fit");

    AppModel::new(72, book)
}

async fn stay_alive() -> ! {
    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}

fn handle_input_event(
    runtime: &mut UiRuntime,
    app: Entity<InkPaperApp>,
    event: InputEvent,
) -> InputAction {
    match event {
        InputEvent::Button(event) => handle_button_event(runtime, event),
        InputEvent::Touch(event) => {
            handle_touch_event(runtime, app, event);
            InputAction::Continue
        }
    }
}

fn handle_button_event(runtime: &mut UiRuntime, event: ButtonEvent) -> InputAction {
    match (event.button(), event.edge()) {
        (Button::Left, ButtonEdge::Pressed) => {
            debug!("input button=left edge=pressed");
            runtime.focus_previous();
            InputAction::Continue
        }
        (Button::Right, ButtonEdge::Pressed) => {
            debug!("input button=right edge=pressed");
            runtime.focus_next();
            InputAction::Continue
        }
        (Button::Power, ButtonEdge::Pressed) => {
            // this will become suspend/awake once the power lifecycle is implemented
            info!("input button=power edge=pressed");
            InputAction::Sleep
        }
        (_, ButtonEdge::Released) => {
            // releases are intentionally retained by the hardware abstraction event
            // though focus navigation doesn't need them yet.
            // they will matter for long press and power handling
            InputAction::Continue
        }
    }
}

fn handle_touch_event(runtime: &mut UiRuntime, app: Entity<InkPaperApp>, event: TouchEvent) {
    match event {
        TouchEvent::Down(position) => {
            debug!("input touch=down x={} y={}", position.x(), position.y(),);
            runtime.begin_activation_at(to_ui_point(position));
        }
        TouchEvent::Up(position) => {
            debug!("input touch=up x={} y={}", position.x(), position.y(),);
            runtime
                .complete_activation_at(to_ui_point(position))
                .unwrap();
        }
        TouchEvent::HomeTap => {
            debug!("input touch=home");
            runtime
                .update(app, |app, cx| {
                    app.navigate(Route::Home, cx);
                })
                .unwrap();
        }
    }
}

fn to_ui_point(position: TouchPosition) -> Point {
    Point::new(px(position.x() as i32), px(position.y() as i32))
}

fn apply_battery_reading(
    runtime: &mut UiRuntime,
    app: Entity<InkPaperApp>,
    reading: BatteryReading,
) {
    debug!(
        "battery update percent={} millivolts={}",
        reading.percent(),
        reading.millivolts(),
    );

    runtime
        .update(app, |app, cx| {
            let old_percent = app.model().battery().value();
            if old_percent == reading.percent() {
                return;
            }

            app.model_mut().set_battery_percent(reading.percent());
            cx.notify();
        })
        .unwrap();
}

fn apply_rtc_state(runtime: &mut UiRuntime, app: Entity<InkPaperApp>, state: RtcState) {
    let result: Result<(), ClockError> = runtime
        .update(app, |app, cx| {
            let was_available = app.model().clock().is_available();

            match state {
                RtcState::Invalid => {
                    let changed = app.model_mut().clock_mut().invalidate();
                    // invalidation: a clock that was visible is worth 1 refresh:
                    // continuing to show a time we now know is untrustworthy is worse
                    // than the refresh
                    if changed && was_available {
                        cx.notify();
                    }

                    Ok(())
                }
                RtcState::Valid(datetime) => {
                    app.model_mut()
                        .clock_mut()
                        .set_utc_time(datetime.hour(), datetime.minute())?;

                    // first valid RTC value after boot gets one refresh so
                    // "--:--" disappears
                    // subsequent minute ticks update only RAM. They will become visible
                    // during the next ordinary UI refresh instead of forcing an e-ink
                    // update every minute
                    if !was_available {
                        cx.notify();
                    }

                    Ok(())
                }
            }
        })
        .unwrap();

    if let Err(error) = result {
        warn!("invalid RTC time received error={:?}", error);
    }
}
