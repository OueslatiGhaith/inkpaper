use embedded_graphics::pixelcolor::Rgb888;
use embedded_graphics_simulator::{
    OutputSettingsBuilder, SimulatorDisplay, SimulatorEvent, Window,
    sdl2::{Keycode, MouseButton},
};
use futures_lite::future;
use inkpaper_app::{AppService, InkPaperApp};
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
                    runtime.begin_activation_at(mouse_position);
                }

                SimulatorEvent::MouseMove { point } => {
                    mouse_position = ui_point(point);

                    if let Some(update) = pointer.move_to(mouse_position) {
                        if update.started {
                            runtime.cancel_activation();
                        }

                        runtime.scroll_at(update.origin, update.delta);
                    }
                }

                SimulatorEvent::MouseButtonUp {
                    mouse_btn: MouseButton::Left,
                    point,
                } => {
                    mouse_position = ui_point(point);

                    match pointer.finish(mouse_position) {
                        PointerRelease::None => {
                            runtime.cancel_activation();
                        }

                        PointerRelease::Tap(position) => {
                            runtime
                                .complete_activation_at(position)
                                .expect("UI activation callback failed");
                        }

                        PointerRelease::Drag { origin, delta } => {
                            runtime.cancel_activation();
                            runtime.scroll_at(origin, delta);
                        }
                    }
                }

                SimulatorEvent::MouseWheel {
                    scroll_delta,
                    direction,
                } => {
                    runtime.scroll_at(mouse_position, wheel_scroll_offset(scroll_delta, direction));
                }

                SimulatorEvent::KeyDown {
                    keycode: Keycode::Up | Keycode::Left,
                    repeat: false,
                    ..
                } => {
                    runtime.focus_previous();
                }

                SimulatorEvent::KeyDown {
                    keycode: Keycode::Down | Keycode::Right | Keycode::Tab,
                    repeat: false,
                    ..
                } => {
                    runtime.focus_next();
                }

                SimulatorEvent::KeyDown {
                    keycode: Keycode::Return | Keycode::Space,
                    repeat: false,
                    ..
                } => {
                    runtime.begin_focused_activation();
                }

                SimulatorEvent::KeyUp {
                    keycode: Keycode::Return | Keycode::Space,
                    ..
                } => {
                    runtime
                        .complete_focused_activation()
                        .expect("UI activation callback failed");
                }

                SimulatorEvent::KeyDown {
                    keycode: Keycode::Home,
                    repeat: false,
                    ..
                } => {
                    runtime
                        .update(app, |app, cx| app.navigate_home(cx))
                        .expect("application root must remain available");
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
