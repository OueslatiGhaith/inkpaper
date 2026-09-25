use defmt::{debug, error, info, warn};
use embassy_executor::Spawner;
use embassy_futures::select::{Either6, select6};
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
use inkpaper_app::{
    AppInputEvent, AppService, BatteryStatus as AppBatteryStatus, ClockStatus as AppClockStatus,
    InkPaperApp, UsbDriveConnection as AppUsbDriveConnection,
};
use inkpaper_ui::prelude::*;
use static_cell::StaticCell;
use xteink_display_probe::{Verdict, detect_x4_controller};

use crate::firmware::{
    auto_sleep::AutoSleep,
    battery::{BATTERY_UPDATES, BatteryReading, battery_task},
    buttons::{Buttons, button_task},
    display::{X4Panel, power::DisplayPowerManager},
    framebuffer::FramebufferStorage,
    frontlight::frontlight_task,
    input::{
        Button, ButtonEdge, INPUT_EVENTS, InputEvent, PowerButtonEvent, TouchEvent, TouchPosition,
    },
    platform::X4Platform,
    power::PowerRails,
    power_button::power_button_task,
    presenter::{Presenter, UiRuntime},
    probe::ProbePins,
    rtc::{RTC_UPDATES, RtcState, rtc_task},
    sleep_pins::release_display_reset_hold,
    suspend::SuspendContext,
    touch::{TouchController, touch_task},
    usb_mass_storage::{USB_HOST_UPDATES, UsbHostState},
};

mod auto_sleep;
mod battery;
mod buttons;
mod display;
mod framebuffer;
mod frontlight;
mod i2c_bus;
mod input;
#[cfg(feature = "trace")]
mod perf;
mod platform;
mod power;
mod power_button;
mod presenter;
mod probe;
mod refresh_policy;
mod rtc;
mod sleep_pins;
mod storage;
mod suspend;
mod touch;
mod usb_mass_storage;

esp_bootloader_esp_idf::esp_app_desc!();

static FRAMEBUFFER: StaticCell<FramebufferStorage> = StaticCell::new();
static UI_RUNTIME: StaticCell<UiRuntime> = StaticCell::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InputAction {
    Continue,
    ForceRefresh,
    Sleep,
}

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    rtt_target::rtt_init_defmt!();
    info!("InkPaper X4 Pro boot");

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    #[cfg(feature = "trace")]
    inkpaper_trace::init(
        esp_hal::xtensa_lx::timer::get_cycle_count,
        perf::CLOCK_HZ,
        perf::trace_monotonic_ticks,
        perf::TRACE_MONOTONIC_HZ,
    );

    esp_alloc::psram_allocator!(peripherals.PSRAM, esp_hal::psram);
    info!("PSRAM allocator initialized");

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
            peripherals.USB_FS,
            peripherals.GPIO20,
            peripherals.GPIO19,
        )
        .unwrap(),
    );

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
            .with_frequency(Rate::from_mhz(10))
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

    let frame = FRAMEBUFFER.init_with(FramebufferStorage::white);

    let runtime = UI_RUNTIME.init_with(UiRuntime::default);

    InkPaperApp::register_resources(runtime).unwrap();
    let app = runtime.create_root(|_| InkPaperApp::default()).unwrap();

    // Storage initialization has been running concurrently with display startup.
    // Wait only when AppService is about to load persisted app state
    let storage_ready = storage::wait_ready().await;
    info!("storage ready: {}", storage_ready);

    let mut app_service = AppService::new(X4Platform::new());
    if app_service.service_pending(runtime, app).await.is_err() {
        warn!("failed to service initial app work");
    }

    let mut presenter = Presenter::default();
    let mut display_power = DisplayPowerManager::default();

    display_power
        .prepare(&mut panel, &mut bus, &mut delay)
        .await
        .unwrap();

    debug!("building UI frame...");
    let update = presenter.render_initial(runtime, frame);
    let damage = update.physical_damage();
    debug!(
        "UI update frame={} refresh={:?} damage_tone={:?} presentation={:?} x={} y={} width={} height={}",
        update.frame_id(),
        update.refresh(),
        update.eink_report().tone(),
        update.presentation(),
        damage.x,
        damage.y,
        damage.width,
        damage.height,
    );

    info!("presenting initial frame...");
    display_power
        .present_initial(&mut panel, &mut bus, &mut delay, frame, update)
        .await
        .unwrap();
    info!("initial display complete");

    if runtime
        .update(app, |app, _| app.request_frontlight_apply())
        .is_err()
    {
        warn!("failed to request initial frontlight state");
    }

    if app_service.service_pending(runtime, app).await.is_err() {
        warn!("failed to apply initial frontlight state");
    }

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

    let mut auto_sleep = AutoSleep::new();

    loop {
        let mut action = InputAction::Continue;

        // main sleeps until either:
        // - physical user input arrives
        // - the battery service has a new reading
        // - the inactivity countdown expires
        // `BATTERY_UPDATES` is a signal, so dropping its pending wait when input
        // wins this select is safe and doesn't lose a stored reading
        match select6(
            INPUT_EVENTS.receive(),
            BATTERY_UPDATES.wait(),
            RTC_UPDATES.wait(),
            display_power.wait_idle_timeout(),
            USB_HOST_UPDATES.wait(),
            auto_sleep.wait(),
        )
        .await
        {
            Either6::First(event) => {
                auto_sleep.reset();

                action = handle_input_event(runtime, app, event);
                // combine events accumulated while the e-ink panel was busy.
                while action == InputAction::Continue
                    && runtime.invalidation() != Invalidation::Rebuild
                {
                    let Ok(event) = INPUT_EVENTS.try_receive() else {
                        break;
                    };
                    action = handle_input_event(runtime, app, event);
                }
            }
            Either6::Second(reading) => apply_battery_reading(runtime, app, reading),
            Either6::Third(state) => apply_rtc_state(runtime, app, state),
            Either6::Fourth(()) => {
                display_power
                    .handle_idle_timeout(&mut panel, &mut bus, &mut delay)
                    .await
                    .unwrap();

                continue;
            }
            Either6::Fifth(state) => apply_usb_host_state(runtime, app, state),
            Either6::Sixth(()) => {
                // re-arm first, so a blocked auto-sleep waits another full timeout
                auto_sleep.reset();

                match runtime.update(app, |app, _| app.allows_auto_sleep()) {
                    Ok(true) => {
                        info!("auto-sleep after inactivity");

                        action = InputAction::Sleep;
                    }
                    Ok(false) => continue,
                    Err(_) => {
                        warn!("failed to query auto-sleep state");

                        continue;
                    }
                }
            }
        }

        if action == InputAction::Sleep {
            suspend::enter(SuspendContext {
                runtime,
                app,
                app_service: &mut app_service,
                presenter: &mut presenter,
                display_power: &mut display_power,
                panel: &mut panel,
                bus: &mut bus,
                delay: &mut delay,
                frame,
                rails: &mut rails,
            })
            .await;
        }

        if action == InputAction::ForceRefresh {
            info!("display: manual full refresh requested");

            match runtime.update(app, |_, cx| cx.notify()) {
                Ok(()) => presenter.request_full_refresh(),
                Err(_) => warn!("failed to invalidate UI for manual full refresh"),
            }
        }

        if !runtime.render_invalidation().is_none() {
            display_power
                .prepare(&mut panel, &mut bus, &mut delay)
                .await
                .unwrap();
        }

        if app_service.service_pending(runtime, app).await.is_err() {
            warn!("failed to service app work");
        }

        // app-service work can itself create an invalidation, and it may also have
        // provided enough time for a previously-busy PowerOff to finish.
        //
        // prepare_present() is idempotent, so trying again is cheap.
        if !runtime.render_invalidation().is_none() {
            display_power
                .prepare(&mut panel, &mut bus, &mut delay)
                .await
                .unwrap();
        }

        let Some(update) = presenter.render_pending(runtime, frame, panel.capabilities()) else {
            continue;
        };

        let damage = update.physical_damage();
        debug!(
            "UI update frame={} refresh={:?} x={} y={} width={} height={}",
            update.frame_id(),
            update.refresh(),
            damage.x,
            damage.y,
            damage.width,
            damage.height,
        );

        display_power
            .present(&mut panel, &mut bus, &mut delay, frame, update)
            .await
            .unwrap();
    }
}

async fn stay_alive() -> ! {
    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}

fn dispatch_app_input(runtime: &mut UiRuntime, app: Entity<InkPaperApp>, event: AppInputEvent) {
    if inkpaper_app::dispatch_input(runtime, app, event).is_err() {
        warn!("failed to dispatch application input");
    }
}

fn handle_input_event(
    runtime: &mut UiRuntime,
    app: Entity<InkPaperApp>,
    event: InputEvent,
) -> InputAction {
    match event {
        InputEvent::Power(PowerButtonEvent::LongPress) => InputAction::Sleep,
        InputEvent::Power(PowerButtonEvent::ShortPress) => InputAction::ForceRefresh,

        InputEvent::Button(event)
            if event.button() == Button::Left && event.edge() == ButtonEdge::Pressed =>
        {
            dispatch_app_input(runtime, app, AppInputEvent::Previous);

            InputAction::Continue
        }

        InputEvent::Button(event)
            if event.button() == Button::Right && event.edge() == ButtonEdge::Pressed =>
        {
            dispatch_app_input(runtime, app, AppInputEvent::Next);

            InputAction::Continue
        }

        InputEvent::Touch(TouchEvent::Down(position)) => {
            dispatch_app_input(runtime, app, AppInputEvent::PointerDown(ui_point(position)));

            InputAction::Continue
        }

        InputEvent::Touch(TouchEvent::Up(position)) => {
            dispatch_app_input(runtime, app, AppInputEvent::PointerUp(ui_point(position)));

            InputAction::Continue
        }

        InputEvent::Touch(TouchEvent::HomeTap) => {
            dispatch_app_input(runtime, app, AppInputEvent::Home);

            InputAction::Continue
        }

        InputEvent::Touch(TouchEvent::Drag {
            origin,
            previous,
            position,
        }) => {
            dispatch_app_input(
                runtime,
                app,
                AppInputEvent::PointerDrag {
                    origin: ui_point(origin),
                    previous: ui_point(previous),
                    position: ui_point(position),
                },
            );

            InputAction::Continue
        }

        _ => InputAction::Continue,
    }
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

    let Some(status) = AppBatteryStatus::new(reading.percent(), reading.millivolts()) else {
        warn!("ignoring invalid battery percentage {}", reading.percent());
        return;
    };

    if runtime
        .update(app, move |app, cx| app.apply_battery_status(status, cx))
        .is_err()
    {
        warn!("failed to apply battery status to app");
    }
}

fn apply_rtc_state(runtime: &mut UiRuntime, app: Entity<InkPaperApp>, state: RtcState) {
    let clock = match state {
        RtcState::Invalid => {
            debug!("rtc update invalid");
            None
        }
        RtcState::Valid(datetime) => {
            debug!(
                "rtc update hour={} minute={}",
                datetime.hour(),
                datetime.minute(),
            );

            let clock = AppClockStatus::new(
                datetime.year(),
                datetime.month(),
                datetime.day(),
                datetime.hour(),
                datetime.minute(),
            );

            if clock.is_none() {
                warn!("RTC produced an invalid application datetime");
            }

            clock
        }
    };

    if runtime
        .update(app, move |app, cx| app.apply_clock_status(clock, cx))
        .is_err()
    {
        warn!("failed to apply RTC status to app");
    }
}

fn apply_usb_host_state(runtime: &mut UiRuntime, app: Entity<InkPaperApp>, state: UsbHostState) {
    let connection = match state {
        UsbHostState::WaitingForHost => AppUsbDriveConnection::WaitingForHost,
        UsbHostState::Connected => AppUsbDriveConnection::Connected,
    };

    if runtime
        .update(app, move |app, cx| {
            app.apply_usb_drive_connection(connection, cx);
        })
        .is_err()
    {
        warn!("failed to apply USB host state to app");
    }
}

fn ui_point(position: TouchPosition) -> Point {
    Point::new(px(i32::from(position.x())), px(i32::from(position.y())))
}
