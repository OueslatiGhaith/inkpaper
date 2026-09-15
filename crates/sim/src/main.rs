use embedded_graphics::{
    geometry::{Point as EgPoint, Size as EgSize},
    pixelcolor::Rgb888,
};
use embedded_graphics_simulator::{
    OutputSettingsBuilder, SimulatorDisplay, SimulatorEvent, Window,
    sdl2::{Keycode, MouseButton},
};
use inkpaper_app::InkPaperApp;
use inkpaper_ui::{
    backend::{CoverageMode, EmbeddedGraphicsPainter},
    prelude::*,
};

use crate::runtime::new_runtime;

mod runtime;

const DISPLAY_WIDTH: u32 = 480;
const DISPLAY_HEIGHT: u32 = 800;

const DISPLAY_SIZE_EG: EgSize = EgSize::new(DISPLAY_WIDTH, DISPLAY_HEIGHT);
const DISPLAY_SIZE: Size = Size::new(px(DISPLAY_WIDTH as i32), px(DISPLAY_HEIGHT as i32));

fn read_simulator_pixel(display: &SimulatorDisplay<Rgb888>, point: EgPoint) -> Option<Rgb888> {
    if point.x < 0
        || point.y < 0
        || point.x >= DISPLAY_WIDTH as i32
        || point.y >= DISPLAY_HEIGHT as i32
    {
        return None;
    }

    Some(display.get_pixel(point))
}

fn paint_ui<R>(runtime: &mut R, display: &mut SimulatorDisplay<Rgb888>, damage: DamageRegion)
where
    R: RenderRuntimeApi,
    for<'target> EmbeddedGraphicsPainter<'target, SimulatorDisplay<Rgb888>>:
        ResourcePainter<R::Resources>,
{
    if damage.is_none() {
        return;
    }

    let mut painter = EmbeddedGraphicsPainter::new(display)
        .with_coverage_mode(CoverageMode::alpha_blend(read_simulator_pixel));

    painter.clear_damage(damage, Color::WHITE).unwrap();

    match runtime.paint_with_damage(damage, &mut painter) {
        Ok(Some(_)) => {}
        Ok(None) => panic!("painting requires a mounted root"),
        Err(_) => panic!("UI painting failed"),
    }
}

fn rebuild_ui<R>(runtime: &mut R, display: &mut SimulatorDisplay<Rgb888>)
where
    R: RenderRuntimeApi,
    for<'target> EmbeddedGraphicsPainter<'target, SimulatorDisplay<Rgb888>>:
        ResourcePainter<R::Resources>,
{
    runtime.rebuild().unwrap();
    runtime.layout(DISPLAY_SIZE).unwrap();

    paint_ui(runtime, display, DamageRegion::full());
}

fn render_pending_ui<R>(runtime: &mut R, display: &mut SimulatorDisplay<Rgb888>)
where
    R: RenderRuntimeApi,
    for<'target> EmbeddedGraphicsPainter<'target, SimulatorDisplay<Rgb888>>:
        ResourcePainter<R::Resources>,
{
    let invalidation = runtime.take_render_invalidation();

    match invalidation.kind() {
        Invalidation::None => return,
        Invalidation::Paint => {}
        Invalidation::Layout => {
            runtime
                .layout(DISPLAY_SIZE)
                .expect("layout requires a mounted root");
        }
        Invalidation::Rebuild => {
            runtime.rebuild().expect("UI rebuild failed");
            runtime
                .layout(DISPLAY_SIZE)
                .expect("rebuilt UI must have a root");
        }
    }

    paint_ui(runtime, display, invalidation.damage());
}

fn ui_point(point: EgPoint) -> Point {
    Point::new(px(point.x), px(point.y))
}

fn main() {
    let mut runtime = new_runtime();

    InkPaperApp::register_resources(&mut runtime).expect("InkPaper resources must fit");

    let app = runtime
        .create_root(|_| InkPaperApp::default())
        .expect("InkPaper application root must fit");

    let mut display = SimulatorDisplay::<Rgb888>::new(DISPLAY_SIZE_EG);

    rebuild_ui(&mut runtime, &mut display);

    let output_settings = OutputSettingsBuilder::new().scale(1).build();

    let mut window = Window::new("InkPaper", &output_settings);

    'running: loop {
        window.update(&display);

        for event in window.events() {
            match event {
                SimulatorEvent::Quit => break 'running,

                SimulatorEvent::MouseButtonDown {
                    mouse_btn: MouseButton::Left,
                    point,
                } => {
                    runtime.begin_activation_at(ui_point(point));
                }

                SimulatorEvent::MouseButtonUp {
                    mouse_btn: MouseButton::Left,
                    point,
                } => {
                    runtime
                        .complete_activation_at(ui_point(point))
                        .expect("UI activation callback failed");
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
                    keycode: Keycode::Escape,
                    repeat: false,
                    ..
                } => {
                    runtime
                        .update(app, |app, cx| app.navigate_home(cx))
                        .expect("application root must remain available");
                }

                _ => {}
            }

            render_pending_ui(&mut runtime, &mut display);
        }
    }
}
