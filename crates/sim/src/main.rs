use embedded_graphics::pixelcolor::Rgb888;
use embedded_graphics_simulator::{
    OutputSettingsBuilder, SimulatorDisplay, SimulatorEvent, Window,
    sdl2::{Keycode, MouseButton},
};
use futures_lite::future;
use std::{
    rc::Rc,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use inkpaper_app::{
    AppInputEvent, AppService, BatteryStatus, ClockStatus, InkPaperApp, TouchGesture,
};
use inkpaper_ui::prelude::*;

use crate::{
    gesture::wheel_input,
    platform::SimulatorPlatform,
    radio::SimulatedRadio,
    render::{DISPLAY_SIZE_EG, render_ui, ui_point},
    runtime::new_runtime,
};

mod fake_fs;
mod gesture;
mod host_epub;
mod platform;
mod radio;
mod render;
mod runtime;

fn main() {
    let mut runtime = new_runtime();

    InkPaperApp::register_resources(&mut runtime).expect("InkPaper resources must fit");

    let app = runtime
        .create_root(InkPaperApp::new)
        .expect("InkPaper application root must fit");

    seed_system_status(&mut runtime, app);

    let radio = Rc::new(SimulatedRadio::default());
    let mut app_service = AppService::new(SimulatorPlatform::new(radio.clone()));
    future::block_on(app_service.service_pending(&mut runtime, app))
        .expect("application service must remain available");

    let mut display = SimulatorDisplay::<Rgb888>::new(DISPLAY_SIZE_EG);

    render_ui(&mut runtime, &mut display);

    let output_settings = OutputSettingsBuilder::new().scale(1).build();

    let mut window = Window::new("InkPaper", &output_settings);

    let started = Instant::now();
    let now_ms = || started.elapsed().as_millis() as u64;

    let mut pointer = TouchGesture::default();

    'running: loop {
        window.update(&display);

        // work the service put off until a frame showed it goes on now that
        // the window shows that frame
        if app_service.has_deferred_work() {
            future::block_on(app_service.service_pending(&mut runtime, app))
                .expect("application service must remain available");
            render_ui(&mut runtime, &mut display);
        }

        if radio.finish_due(&mut runtime, app) {
            render_ui(&mut runtime, &mut display);
        }

        // a mouse button held still long presses
        if let Some(input) = pointer.tick(now_ms()) {
            send_input(&mut runtime, app, input);
            future::block_on(app_service.service_pending(&mut runtime, app))
                .expect("application service must remain available");
            render_ui(&mut runtime, &mut display);
        }

        for event in window.events() {
            match event {
                SimulatorEvent::Quit => {
                    // closing the window is the device going to sleep, so the
                    // book being read opens again at the next start. The clock
                    // doesn't tick in the simulator, so it's read again
                    let _ = runtime.update(app, |app, cx| {
                        app.apply_clock_status(Some(host_clock()), cx);
                    });
                    let _ = app_service.prepare_for_sleep(&mut runtime, app);
                    let _ = future::block_on(app_service.flush());
                    break 'running;
                }

                SimulatorEvent::MouseButtonDown {
                    mouse_btn: MouseButton::Left,
                    point,
                } => {
                    // a press that was never released starts over
                    pointer.release();

                    if let Some(input) = pointer.touch(ui_point(point), now_ms()) {
                        send_input(&mut runtime, app, input);
                    }
                }

                SimulatorEvent::MouseMove { point } => {
                    if pointer.is_touching()
                        && let Some(input) = pointer.touch(ui_point(point), now_ms())
                    {
                        send_input(&mut runtime, app, input);
                    }
                }

                SimulatorEvent::MouseButtonUp {
                    mouse_btn: MouseButton::Left,
                    point,
                } => {
                    if pointer.is_touching()
                        && let Some(input) = pointer.touch(ui_point(point), now_ms())
                    {
                        send_input(&mut runtime, app, input);
                    }

                    let input = pointer.release().unwrap_or(AppInputEvent::PointerCancel);
                    send_input(&mut runtime, app, input);
                }

                SimulatorEvent::MouseWheel {
                    scroll_delta,
                    direction,
                } => {
                    if let Some(input) = wheel_input(scroll_delta, direction) {
                        send_input(&mut runtime, app, input);
                    }
                }

                SimulatorEvent::KeyDown {
                    keycode: Keycode::Left,
                    repeat: false,
                    ..
                } => {
                    send_input(&mut runtime, app, AppInputEvent::Previous);
                }

                SimulatorEvent::KeyDown {
                    keycode: Keycode::Right,
                    repeat: false,
                    ..
                } => {
                    send_input(&mut runtime, app, AppInputEvent::Next);
                }

                SimulatorEvent::KeyDown {
                    keycode: Keycode::H,
                    repeat: false,
                    ..
                } => {
                    send_input(&mut runtime, app, AppInputEvent::Home);
                }

                _ => {}
            }

            future::block_on(app_service.service_pending(&mut runtime, app))
                .expect("application service must remain available");

            // the simulator can persist immediately. On the X4 we will instead flush
            // reading progress at suspend so page turns do not write the SD card continuously.
            let _ = future::block_on(app_service.flush());

            render_ui(&mut runtime, &mut display);
        }
    }
}

fn seed_system_status(runtime: &mut impl RuntimeApi, app: Entity<InkPaperApp>) {
    let battery = BatteryStatus::new(72, 3_880, false).expect("simulated battery must be valid");

    runtime
        .update(app, move |app, cx| {
            app.apply_battery_status(battery, cx);
            app.apply_clock_status(Some(host_clock()), cx);
        })
        .expect("application root must remain available");
}

/// The host's time in UTC, like the X4's RTC.
fn host_clock() -> ClockStatus {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("host clock must be after 1970")
        .as_secs();
    let (days, minute_of_day) = (seconds / 86_400, seconds % 86_400 / 60);

    // Howard Hinnant's civil_from_days, with March first so the leap day ends
    // the year
    let days = days as i64 + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = (month_index + 2) % 12 + 1;
    let year = year_of_era + era * 400 + i64::from(month <= 2);

    ClockStatus::new(
        year as u16,
        month as u8,
        day as u8,
        (minute_of_day / 60) as u8,
        (minute_of_day % 60) as u8,
    )
    .expect("host clock must be a valid date")
}

fn send_input(runtime: &mut impl RuntimeApi, app: Entity<InkPaperApp>, event: AppInputEvent) {
    inkpaper_app::dispatch_input(runtime, app, event)
        .expect("application input dispatch must remain available");
}
