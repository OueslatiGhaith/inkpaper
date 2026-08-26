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
use inkpaper_app::{
    AppEvent, AppModel, BookSummary, Button as AppButton, ButtonEdge as AppButtonEdge,
    ButtonEvent as AppButtonEvent, InkPaperApp, InputEvent as AppInputEvent, PlatformAction,
    ScrollEvent as AppScrollEvent, TouchEvent as AppTouchEvent, TouchPosition as AppTouchPosition,
    theme::Theme,
};
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
    let current_book = BookSummary::try_new("The Left Hand of Darkness", "Ursula K. Le Guin", 68)
        .expect("demo book metadata must fit");

    let mut model = AppModel::new(73, current_book.clone());

    let books = [
        ("/Books/The Left Hand of Darkness.epub", current_book),
        (
            "/Books/Dune.epub",
            BookSummary::try_new("Dune", "Frank Herbert", 21).unwrap(),
        ),
        (
            "/Books/The Dispossessed.epub",
            BookSummary::try_new("The Dispossessed", "Ursula K. Le Guin", 0).unwrap(),
        ),
        (
            "/Books/Foundation.epub",
            BookSummary::try_new("Foundation", "Isaac Asimov", 84).unwrap(),
        ),
        (
            "/Books/Neuromancer.epub",
            BookSummary::try_new("Neuromancer", "William Gibson", 12).unwrap(),
        ),
        (
            "/Books/Blindsight.epub",
            BookSummary::try_new("Blindsight", "Peter Watts", 0).unwrap(),
        ),
    ];

    for (path, summary) in books {
        model
            .library_mut()
            .try_push(
                inkpaper_app::LibraryEntry::try_new(path, summary)
                    .expect("demo book path must fit"),
            )
            .expect("demo library must fit one page");
    }

    model
}

fn to_touch_position(point: EgPoint) -> AppTouchPosition {
    let x = point.x.clamp(0, DISPLAY_WIDTH as i32 - 1);
    let y = point.y.clamp(0, DISPLAY_HEIGHT as i32 - 1);

    AppTouchPosition::new(x as u16, y as u16)
}

fn button_event(button: AppButton, edge: AppButtonEdge) -> AppEvent {
    AppEvent::Input(AppInputEvent::Button(AppButtonEvent::new(button, edge)))
}

fn key_event(keycode: Keycode, edge: AppButtonEdge) -> Option<AppEvent> {
    let button = match keycode {
        Keycode::Down | Keycode::Right | Keycode::Tab => AppButton::Next,
        Keycode::Up | Keycode::Left => AppButton::Previous,
        Keycode::Return | Keycode::Space => AppButton::Activate,
        // P represents the physical power button in the simulator.
        // The application decides that pressing it requests suspend.
        Keycode::P => AppButton::Power,
        _ => return None,
    };

    Some(button_event(button, edge))
}

fn handle_app_event(
    runtime: &mut UiRuntime,
    app: Entity<InkPaperApp>,
    event: AppEvent,
) -> PlatformAction {
    InkPaperApp::handle_event(runtime, app, event)
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

    let mut mouse_position = AppTouchPosition::new(0, 0);

    'running: loop {
        window.update(&display);

        for event in window.events() {
            let action = match event {
                SimulatorEvent::Quit => break 'running,
                SimulatorEvent::MouseMove { point } => {
                    mouse_position = to_touch_position(point);
                    PlatformAction::None
                }
                SimulatorEvent::MouseButtonDown {
                    mouse_btn: MouseButton::Left,
                    point,
                } => {
                    mouse_position = to_touch_position(point);
                    handle_app_event(
                        &mut runtime,
                        app,
                        AppEvent::Input(AppInputEvent::Touch(AppTouchEvent::Down(mouse_position))),
                    )
                }
                SimulatorEvent::MouseButtonUp {
                    mouse_btn: MouseButton::Left,
                    point,
                } => {
                    mouse_position = to_touch_position(point);
                    handle_app_event(
                        &mut runtime,
                        app,
                        AppEvent::Input(AppInputEvent::Touch(AppTouchEvent::Up(mouse_position))),
                    )
                }
                SimulatorEvent::MouseWheel { scroll_delta, .. } => {
                    const SCROLL_STEP: i32 = 18;
                    let delta_x = -scroll_delta.x.saturating_mul(SCROLL_STEP);
                    let delta_y = -scroll_delta.y.saturating_mul(SCROLL_STEP);
                    handle_app_event(
                        &mut runtime,
                        app,
                        AppEvent::Input(AppInputEvent::Scroll(AppScrollEvent::new(
                            mouse_position,
                            delta_x,
                            delta_y,
                        ))),
                    )
                }
                SimulatorEvent::KeyDown {
                    keycode,
                    repeat: false,
                    ..
                } => match key_event(keycode, AppButtonEdge::Pressed) {
                    Some(event) => handle_app_event(&mut runtime, app, event),
                    None => PlatformAction::None,
                },
                SimulatorEvent::KeyUp { keycode, .. } => {
                    match key_event(keycode, AppButtonEdge::Released) {
                        Some(event) => handle_app_event(&mut runtime, app, event),
                        None => PlatformAction::None,
                    }
                }
                _ => PlatformAction::None,
            };

            match action {
                PlatformAction::None => {}
                PlatformAction::Suspend => {
                    // the real X4 platform performs its complete deep-sleep sequence.
                    break 'running;
                }
            }
        }

        update_ui(&mut runtime, app, &mut display);
    }
}
