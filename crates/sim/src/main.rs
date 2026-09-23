use embedded_graphics::pixelcolor::Rgb888;
use embedded_graphics_simulator::{
    OutputSettingsBuilder, SimulatorDisplay, SimulatorEvent, Window,
    sdl2::{Keycode, MouseButton},
};
use futures_lite::future;
use inkpaper_app::{AppInputEvent, AppService, BatteryStatus, ClockStatus, InkPaperApp};
use inkpaper_ui::prelude::*;

use crate::{
    gesture::{PointerGesture, PointerRelease, wheel_scroll_offset},
    platform::SimulatorPlatform,
    render::{
        DISPLAY_HEIGHT, DISPLAY_SIZE_EG, DISPLAY_WIDTH, rebuild_ui, render_pending_ui, ui_point,
    },
    runtime::new_runtime,
};

mod fake_fs;
mod gesture;
mod host_epub;
mod platform;
mod render;
mod runtime;

fn main() {
    let mut runtime = new_runtime();

    InkPaperApp::register_resources(&mut runtime).expect("InkPaper resources must fit");

    let app = runtime
        .create_root(|_| InkPaperApp::default())
        .expect("InkPaper application root must fit");

    seed_system_status(&mut runtime, app);

    let mut app_service = AppService::new(SimulatorPlatform::new());
    future::block_on(app_service.service_pending(&mut runtime, app))
        .expect("application service must remain available");

    let mut display = SimulatorDisplay::<Rgb888>::new(DISPLAY_SIZE_EG);

    rebuild_ui(&mut runtime, &mut display);

    let output_settings = OutputSettingsBuilder::new().scale(1).build();

    let mut window = Window::new("InkPaper", &output_settings);

    let mut pointer = PointerGesture::default();
    let mut mouse_position =
        Point::new(px(DISPLAY_WIDTH as i32 / 2), px(DISPLAY_HEIGHT as i32 / 2));

    'running: loop {
        window.update(&display);

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

                    pointer.begin(mouse_position);
                    send_input(
                        &mut runtime,
                        app,
                        AppInputEvent::PointerDown(mouse_position),
                    );
                }

                SimulatorEvent::MouseMove { point } => {
                    mouse_position = ui_point(point);

                    if let Some(update) = pointer.move_to(mouse_position) {
                        send_input(
                            &mut runtime,
                            app,
                            AppInputEvent::PointerDrag {
                                origin: update.origin,
                                previous: update.previous,
                                position: update.position,
                            },
                        );
                    }
                }

                SimulatorEvent::MouseButtonUp {
                    mouse_btn: MouseButton::Left,
                    point,
                } => {
                    mouse_position = ui_point(point);

                    match pointer.finish(mouse_position) {
                        PointerRelease::None => {
                            send_input(&mut runtime, app, AppInputEvent::PointerCancel);
                        }

                        PointerRelease::Tap(position) => {
                            send_input(&mut runtime, app, AppInputEvent::PointerUp(position));
                        }

                        PointerRelease::Drag {
                            origin,
                            previous,
                            position,
                        } => {
                            if previous != position {
                                send_input(
                                    &mut runtime,
                                    app,
                                    AppInputEvent::PointerDrag {
                                        origin,
                                        previous,
                                        position,
                                    },
                                );
                            }

                            send_input(&mut runtime, app, AppInputEvent::PointerUp(position));
                        }
                    }
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
                    keycode: Keycode::Up,
                    repeat: false,
                    ..
                } => {
                    send_input(&mut runtime, app, AppInputEvent::FocusPrevious);
                }

                SimulatorEvent::KeyDown {
                    keycode: Keycode::Down | Keycode::Tab,
                    repeat: false,
                    ..
                } => {
                    send_input(&mut runtime, app, AppInputEvent::FocusNext);
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
                    keycode: Keycode::Return | Keycode::Space,
                    repeat: false,
                    ..
                } => {
                    send_input(&mut runtime, app, AppInputEvent::ConfirmDown);
                }

                SimulatorEvent::KeyUp {
                    keycode: Keycode::Return | Keycode::Space,
                    ..
                } => {
                    send_input(&mut runtime, app, AppInputEvent::ConfirmUp);
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
