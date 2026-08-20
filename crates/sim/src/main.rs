use embedded_graphics::{
    geometry::{Point as EgPoint, Size as EgSize},
    mono_font::{
        MonoFont,
        ascii::{FONT_6X10, FONT_10X20},
    },
    pixelcolor::Rgb888,
};
use embedded_graphics_simulator::{
    OutputSettingsBuilder, SimulatorDisplay, SimulatorEvent, Window,
    sdl2::{Keycode, MouseButton},
};
use inkpaper_app::{AppModel, BookSummary, InkPaperApp, theme::Theme};
use inkpaper_ui::{backend::EmbeddedGraphicsPainter, prelude::*};

const DISPLAY_WIDTH: u32 = 480;
const DISPLAY_HEIGHT: u32 = 800;

const DISPLAY_SIZE_EG: EgSize = EgSize::new(DISPLAY_WIDTH, DISPLAY_HEIGHT);

const DISPLAY_SIZE: Size = Size::new(px(DISPLAY_WIDTH as i32), px(DISPLAY_HEIGHT as i32));

const FONTS: [&MonoFont; 2] = [&FONT_6X10, &FONT_10X20];

type UiRuntime = Runtime<
    16_384, // entity bytes
    32,     // entity slots
    8_192,  // callback bytes
    64,     // callback slots
    512,    // frame nodes
    8_192,  // frame text bytes
    256,    // persistent element states
    2_048,  // global btes
    8,      //global slots
>;

fn demo_model() -> AppModel {
    let book = BookSummary::try_new("The Left Hand of Darkness", "Ursula K. Le Guin", 68)
        .expect("demo book metadata must fit");

    AppModel::new(73, book)
}

fn to_ui_point(point: EgPoint) -> Point {
    Point::new(px(point.x), px(point.y))
}

fn paint_ui(runtime: &mut UiRuntime, display: &mut SimulatorDisplay<Rgb888>, damage: DamageRegion) {
    if damage.is_none() {
        return;
    }

    let mut painter = EmbeddedGraphicsPainter::new(display, FONTS, []);

    painter.clear_damage(damage, Color::WHITE).unwrap();

    runtime
        .paint_with_damage(damage, &mut painter)
        .unwrap()
        .unwrap();
}

fn rebuild_ui(
    runtime: &mut UiRuntime,
    app: Entity<InkPaperApp>,
    display: &mut SimulatorDisplay<Rgb888>,
) {
    runtime.rebuild(app).unwrap();
    layout_ui(runtime, display);
    paint_ui(runtime, display, DamageRegion::full());
}

fn update_ui(
    runtime: &mut UiRuntime,
    app: Entity<InkPaperApp>,
    display: &mut SimulatorDisplay<Rgb888>,
) {
    let invalidation = runtime.take_render_invalidation();

    match invalidation.kind() {
        Invalidation::None => {}
        Invalidation::Paint => {
            paint_ui(runtime, display, invalidation.damage());
        }
        Invalidation::Layout => {
            layout_ui(runtime, display);
            paint_ui(runtime, display, invalidation.damage());
        }
        Invalidation::Rebuild => {
            rebuild_ui(runtime, app, display);
        }
    }
}

fn handle_key_down(runtime: &mut UiRuntime, keycode: Keycode) {
    match keycode {
        Keycode::Down | Keycode::Right | Keycode::Tab => {
            runtime.focus_next();
        }
        Keycode::Up | Keycode::Left => {
            runtime.focus_previous();
        }
        Keycode::Return | Keycode::Space => {
            runtime.begin_focused_activation();
        }
        Keycode::C => {
            runtime.cancel_activation();
            runtime.clear_focus();
        }
        _ => {}
    }
}

fn handle_key_up(runtime: &mut UiRuntime, keycode: Keycode) {
    match keycode {
        Keycode::Return | Keycode::Space => {
            runtime.complete_focused_activation().unwrap();
        }
        _ => {}
    }
}

fn layout_ui(runtime: &mut UiRuntime, display: &mut SimulatorDisplay<Rgb888>) {
    let painter = EmbeddedGraphicsPainter::new(display, FONTS, []);

    runtime.layout(DISPLAY_SIZE, &painter).unwrap();
}

fn main() {
    let mut runtime = UiRuntime::default();
    runtime.set_global(Theme::EINK).unwrap();
    let model = demo_model();
    let app = runtime.create(move |_| InkPaperApp::new(model)).unwrap();

    let mut display = SimulatorDisplay::<Rgb888>::new(DISPLAY_SIZE_EG);

    rebuild_ui(&mut runtime, app, &mut display);

    let output_settings = OutputSettingsBuilder::new().scale(1).build();
    let mut window = Window::new("InkPaper X4 Pro", &output_settings);

    let mut mouse_position = Point::ZERO;

    'running: loop {
        window.update(&display);

        for event in window.events() {
            match event {
                SimulatorEvent::Quit => break 'running,
                SimulatorEvent::MouseMove { point } => mouse_position = to_ui_point(point),
                SimulatorEvent::MouseButtonDown {
                    mouse_btn: MouseButton::Left,
                    point,
                } => {
                    mouse_position = to_ui_point(point);
                    runtime.begin_activation_at(mouse_position);
                }
                SimulatorEvent::MouseButtonUp {
                    mouse_btn: MouseButton::Left,
                    point,
                } => {
                    mouse_position = to_ui_point(point);
                    runtime.complete_activation_at(mouse_position).unwrap();
                }
                SimulatorEvent::MouseWheel { scroll_delta, .. } => {
                    const SCROLL_STEP: i32 = 18;

                    runtime.scroll_at(
                        mouse_position,
                        Offset::new(
                            px(-scroll_delta.x.saturating_mul(SCROLL_STEP)),
                            px(-scroll_delta.y.saturating_mul(SCROLL_STEP)),
                        ),
                    );
                }
                SimulatorEvent::KeyDown {
                    keycode,
                    repeat: false,
                    ..
                } => handle_key_down(&mut runtime, keycode),
                SimulatorEvent::KeyUp { keycode, .. } => handle_key_up(&mut runtime, keycode),
                _ => {}
            }
        }

        update_ui(&mut runtime, app, &mut display);
    }
}
