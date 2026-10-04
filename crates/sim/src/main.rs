use embedded_graphics::pixelcolor::Rgb888;
use embedded_graphics_simulator::{
    OutputSettingsBuilder, SimulatorDisplay, SimulatorEvent, Window,
    sdl2::{Keycode, MouseButton},
};
use futures_lite::future;
use std::{rc::Rc, time::Instant};

use inkpaper_app::{
    AppInputEvent, AppService, BatteryStatus, ClockStatus, InkPaperApp, TouchGesture,
};
use inkpaper_ui::prelude::*;

use crate::{
    gesture::wheel_scroll_offset,
    platform::SimulatorPlatform,
    radio::SimulatedRadio,
    render::{
        DISPLAY_HEIGHT, DISPLAY_SIZE_EG, DISPLAY_WIDTH, rebuild_ui, render_pending_ui, ui_point,
    },
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

    rebuild_ui(&mut runtime, &mut display);

    let output_settings = OutputSettingsBuilder::new().scale(1).build();

    let mut window = Window::new("InkPaper", &output_settings);

    let started = Instant::now();
    let now_ms = || started.elapsed().as_millis() as u64;

    let mut pointer = TouchGesture::default();
    let mut mouse_position =
        Point::new(px(DISPLAY_WIDTH as i32 / 2), px(DISPLAY_HEIGHT as i32 / 2));

    'running: loop {
        window.update(&display);

        if radio.finish_due(&mut runtime, app) {
            render_pending_ui(&mut runtime, &mut display);
        }

        // a mouse button held still long presses
        if let Some(input) = pointer.tick(now_ms()) {
            send_input(&mut runtime, app, input);
            render_pending_ui(&mut runtime, &mut display);
        }

        for event in window.events() {
            match event {
                SimulatorEvent::Quit => {
                    let _ = future::block_on(app_service.flush());
                    break 'running;
                }

                SimulatorEvent::MouseButtonDown {
                    mouse_btn: MouseButton::Left,
                    point,
                } => {
                    mouse_position = ui_point(point);

                    // a press that was never released starts over
                    pointer.release();

                    if let Some(input) = pointer.touch(mouse_position, now_ms()) {
                        send_input(&mut runtime, app, input);
                    }
                }

                SimulatorEvent::MouseMove { point } => {
                    mouse_position = ui_point(point);

                    if pointer.is_touching()
                        && let Some(input) = pointer.touch(mouse_position, now_ms())
                    {
                        send_input(&mut runtime, app, input);
                    }
                }

                SimulatorEvent::MouseButtonUp {
                    mouse_btn: MouseButton::Left,
                    point,
                } => {
                    mouse_position = ui_point(point);

                    if pointer.is_touching()
                        && let Some(input) = pointer.touch(mouse_position, now_ms())
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
                    send_input(
                        &mut runtime,
                        app,
                        AppInputEvent::ScrollWheel {
                            position: mouse_position,
                            delta: wheel_scroll_offset(scroll_delta, direction),
                        },
                    );
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

            render_pending_ui(&mut runtime, &mut display);
        }
    }
}

fn seed_system_status(runtime: &mut impl RuntimeApi, app: Entity<InkPaperApp>) {
    let battery = BatteryStatus::new(72, 3_880).expect("simulated battery must be valid");
    let clock = ClockStatus::new(2026, 9, 23, 12, 34).expect("simulated clock must be valid");

    runtime
        .update(app, move |app, cx| {
            app.apply_battery_status(battery, cx);
            app.apply_clock_status(Some(clock), cx);
        })
        .expect("application root must remain available");
}

fn send_input(runtime: &mut impl RuntimeApi, app: Entity<InkPaperApp>, event: AppInputEvent) {
    inkpaper_app::dispatch_input(runtime, app, event)
        .expect("application input dispatch must remain available");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn painted_home() -> (
        runtime::SimulatorRuntime<'static>,
        Entity<InkPaperApp>,
        SimulatorDisplay<Rgb888>,
    ) {
        let mut runtime = new_runtime();
        InkPaperApp::register_resources(&mut runtime).unwrap();
        let app = runtime.create_root(InkPaperApp::new).unwrap();
        seed_system_status(&mut runtime, app);

        let mut display = SimulatorDisplay::new(DISPLAY_SIZE_EG);
        rebuild_ui(&mut runtime, &mut display);

        (runtime, app, display)
    }

    /// paints the current frame from scratch
    fn repaint(runtime: &mut runtime::SimulatorRuntime<'static>) -> SimulatorDisplay<Rgb888> {
        let mut display = SimulatorDisplay::new(DISPLAY_SIZE_EG);
        rebuild_ui(runtime, &mut display);

        display
    }

    #[test]
    fn a_partially_painted_battery_change_matches_a_full_repaint() {
        let (mut runtime, app, mut display) = painted_home();

        runtime
            .update(app, |app, cx| {
                app.apply_battery_status(BatteryStatus::new(9, 3_500).unwrap(), cx);
            })
            .unwrap();
        render_pending_ui(&mut runtime, &mut display);

        // only the header's indicator was painted again
        assert!(!runtime.rebuild_damage().is_full());
        assert!(display == repaint(&mut runtime));
    }

    #[test]
    fn a_clock_tick_on_the_home_screen_paints_nothing() {
        let (mut runtime, app, mut display) = painted_home();
        let before = repaint(&mut runtime);

        runtime
            .update(app, |app, cx| {
                app.apply_clock_status(ClockStatus::new(2026, 9, 23, 12, 35), cx);
            })
            .unwrap();
        render_pending_ui(&mut runtime, &mut display);

        assert!(display == before);
    }
}
