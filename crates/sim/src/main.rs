use embedded_graphics::{
    geometry::{Point as EgPoint, Size as EgSize},
    pixelcolor::Rgb888,
};
use embedded_graphics_simulator::{
    OutputSettingsBuilder, SimulatorDisplay, SimulatorEvent, Window,
    sdl2::{Keycode, MouseButton},
};
use futures_lite::future;
use inkpaper_app::{
    BrowseEntry, BrowseListing, BrowseRequest, InkPaperApp, ReaderDocument, ReaderRequest,
    load_reader_document,
};
use inkpaper_ui::{
    backend::{CoverageMode, EmbeddedGraphicsPainter},
    prelude::*,
};

use crate::{
    gesture::{PointerGesture, PointerRelease, wheel_scroll_offset},
    host_epub::HostFileSource,
    runtime::new_runtime,
};

mod gesture;
mod host_epub;
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

fn service_app_requests(runtime: &impl RuntimeApi, app: Entity<InkPaperApp>) {
    loop {
        let (browse_request, reader_request) = runtime
            .update(app, |app, _| {
                (app.take_browse_request(), app.take_reader_request())
            })
            .expect("app root must be available");

        if browse_request.is_none() && reader_request.is_none() {
            return;
        }

        if let Some(request) = browse_request {
            match request {
                BrowseRequest::ListDirectory(path) => match simulator_listing(&path) {
                    Some(listing) => {
                        runtime
                            .update(app, move |app, cx| {
                                app.apply_browse_listing(listing, cx);
                            })
                            .expect("app root must be available");
                    }
                    None => {
                        runtime
                            .update(app, |app, cx| {
                                app.apply_browse_error(cx);
                            })
                            .expect("app root must be available");
                    }
                },
            }
        }

        if let Some(request) = reader_request {
            match request {
                ReaderRequest::OpenEpub(path) => match simulator_reader_document(path.clone()) {
                    Some(document) => {
                        runtime
                            .update(app, move |app, cx| {
                                app.apply_reader_document(document, cx);
                            })
                            .expect("app root must be available");
                    }
                    None => {
                        runtime
                            .update(app, move |app, cx| {
                                app.apply_reader_error(path, cx);
                            })
                            .expect("app root must be available");
                    }
                },
            }
        }
    }
}

fn simulator_listing(path: &str) -> Option<BrowseListing> {
    let entries = match path {
        "/" => vec![
            BrowseEntry::directory("Books"),
            BrowseEntry::directory("Documents"),
            BrowseEntry::directory("Fixtures"),
            BrowseEntry::directory("Read"),
            BrowseEntry::file("A Fire Upon the Deep.epub"),
            BrowseEntry::file("Blindsight.epub"),
            BrowseEntry::file("Children of Time.epub"),
            BrowseEntry::file("Dune.epub"),
            BrowseEntry::file("Foundation.epub"),
            BrowseEntry::file("Hyperion.epub"),
            BrowseEntry::file("Neuromancer.epub"),
            BrowseEntry::file("Project Hail Mary.epub"),
            BrowseEntry::file("Snow Crash.epub"),
            BrowseEntry::file("The Dispossessed.epub"),
            BrowseEntry::file("The Left Hand of Darkness.epub"),
            BrowseEntry::file("The Three-Body Problem.epub"),
        ],

        "/Books" => vec![
            BrowseEntry::directory("Sci-Fi"),
            BrowseEntry::file("Neuromancer.epub"),
            BrowseEntry::file("The Dispossessed.epub"),
            BrowseEntry::file("Hyperion.epub"),
        ],

        "/Books/Sci-Fi" => vec![
            BrowseEntry::file("Children of Time.epub"),
            BrowseEntry::file("The Left Hand of Darkness.epub"),
            BrowseEntry::file("Foundation.epub"),
        ],

        "/Documents" => vec![
            BrowseEntry::file("Distributed Systems.pdf"),
            BrowseEntry::file("Cloud Computing.pdf"),
        ],

        "/Fixtures" => vec![
            BrowseEntry::file("book-boundaries.epub"),
            BrowseEntry::file("broken-chapter.epub"),
            BrowseEntry::file("broken-image.epub"),
        ],

        "/Read" => vec![],

        _ => return None,
    };

    Some(BrowseListing::new(path, entries))
}

fn simulator_reader_document(path: String) -> Option<ReaderDocument> {
    let file_name = path.strip_prefix("/Fixtures/")?;

    if !matches!(
        file_name,
        "book-boundaries.epub" | "broken-chapter.epub" | "broken-image.epub"
    ) {
        return None;
    }

    let host_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(file_name);

    let source = HostFileSource::open(&host_path).ok()?;

    future::block_on(load_reader_document(path, source)).ok()
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

    let mut pointer = PointerGesture::default();
    let mut mouse_position =
        Point::new(px(DISPLAY_WIDTH as i32 / 2), px(DISPLAY_HEIGHT as i32 / 2));

    'running: loop {
        window.update(&display);

        for event in window.events() {
            match event {
                SimulatorEvent::Quit => break 'running,

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

            service_app_requests(&runtime, app);
            render_pending_ui(&mut runtime, &mut display);
        }
    }
}
