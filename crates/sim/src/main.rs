use std::{fs::File, io::Read, path::Path};

use clap::Parser;
use embedded_graphics::{
    geometry::{Point as EgPoint, Size as EgSize},
    mono_font::ascii::{FONT_6X10, FONT_10X20},
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
use inkpaper_ui::{
    FontData, FontFace, FontRegistry, TtfFont,
    backend::{CoverageMode, EmbeddedGraphicsPainter, MonoFontFace},
    prelude::*,
};
use static_cell::StaticCell;

use crate::{args::SimulatorArgs, host_image::HostImage, reader_demo::load_reader_preview};

mod args;
mod host_epub;
mod host_image;
mod reader_demo;

const DISPLAY_WIDTH: u32 = 480;
const DISPLAY_HEIGHT: u32 = 800;

const DISPLAY_SIZE_EG: EgSize = EgSize::new(DISPLAY_WIDTH, DISPLAY_HEIGHT);
const DISPLAY_SIZE: Size = Size::new(px(DISPLAY_WIDTH as i32), px(DISPLAY_HEIGHT as i32));

const RUNTIME_FONT_STORAGE_BYTES: usize = 5 * 1024 * 1024;

static RUNTIME_FONT_BYTES: StaticCell<Box<[u8; RUNTIME_FONT_STORAGE_BYTES]>> = StaticCell::new();
static RUNTIME_FONT: StaticCell<TtfFont> = StaticCell::new();

static BODY_FONT: MonoFontFace<'static> = MonoFontFace::ascii(&FONT_6X10);
static HEADING_FONT: MonoFontFace<'static> = MonoFontFace::ascii(&FONT_10X20);

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

fn runtime_font_path(path: Option<&Path>) -> Option<&'static TtfFont<'static>> {
    let path = path?;

    let data = load_font_data(path);
    let font = TtfFont::parse(data, 0).unwrap_or_else(|error| {
        panic!(
            "{} is not supported TTF/OTF face: {error:?}",
            path.display()
        )
    });

    let font = RUNTIME_FONT.init(font);

    eprintln!("runtime font: {} ({} bytes)", path.display(), data.len());

    Some(font)
}

fn load_font_data(path: &Path) -> FontData<'static> {
    let mut file = File::open(path)
        .unwrap_or_else(|error| panic!("failed to open font {}: {error}", path.display()));
    let metadata = file
        .metadata()
        .unwrap_or_else(|error| panic!("failed to stat font {}: {error}", path.display()));
    let file_len = usize::try_from(metadata.len()).expect("font file must fit usize");

    assert!(file_len > 0, "font {} is empty", path.display());
    assert!(
        file_len <= RUNTIME_FONT_STORAGE_BYTES,
        "font {} is {file_len} bytes, but simulator storage is limited to {RUNTIME_FONT_STORAGE_BYTES} bytes",
        path.display()
    );

    let storage = RUNTIME_FONT_BYTES.init_with(|| Box::new([0; RUNTIME_FONT_STORAGE_BYTES]));
    file.read_exact(&mut storage[..file_len])
        .unwrap_or_else(|error| panic!("failed to read font {}: {error}", path.display()));

    FontData::new(&storage[..file_len])
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
    runtime: &mut impl RuntimeApi,
    app: Entity<InkPaperApp>,
    event: AppEvent,
) -> PlatformAction {
    InkPaperApp::handle_event(runtime, app, event)
}

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
    layout_ui(runtime);
    paint_ui(runtime, display, DamageRegion::full());
}

fn update_ui<R>(runtime: &mut R, display: &mut SimulatorDisplay<Rgb888>)
where
    R: RenderRuntimeApi,
    for<'target> EmbeddedGraphicsPainter<'target, SimulatorDisplay<Rgb888>>:
        ResourcePainter<R::Resources>,
{
    let invalidation = runtime.take_render_invalidation();

    match invalidation.kind() {
        Invalidation::None => {}
        Invalidation::Paint => {
            paint_ui(runtime, display, invalidation.damage());
        }
        Invalidation::Layout => {
            layout_ui(runtime);
            paint_ui(runtime, display, invalidation.damage());
        }
        Invalidation::Rebuild => {
            rebuild_ui(runtime, display);
        }
    }
}

fn layout_ui(runtime: &mut impl RenderRuntimeApi) {
    runtime.layout(DISPLAY_SIZE).unwrap();
}

fn main() {
    let args = SimulatorArgs::parse();

    let mut runtime = RuntimeBuilder::default()
        .entities::<16_384, 32>()
        .callbacks::<8_192, 64>()
        .frame::<2_048, 32_768>()
        .element_states::<256>()
        .globals::<2_048, 8>()
        .render_resources::<2, 128, { 16 * 1024 }, 1>()
        .build();
    runtime.set_global(Theme::EINK).unwrap();

    let runtime_font = runtime_font_path(args.font.as_deref());
    let runtime_font = runtime_font.map(|font| font as &'static dyn FontFace);

    // reader pagination needs a FontRegistry independently of the runtime.
    // register the exact same faces in the same order so FontIds are identical
    // between pagination and painting.
    let mut reader_fonts = FontRegistry::<2>::default();
    let (reader_body_font, reader_heading_font) = if let Some(runtime_font) = runtime_font {
        let runtime_id = runtime
            .register_font(runtime_font)
            .expect("runtime font slot must fit");

        let reader_id = reader_fonts
            .register(runtime_font)
            .expect("reader font slot must fit");

        assert_eq!(runtime_id, reader_id);

        (reader_id, reader_id)
    } else {
        let runtime_body = runtime.register_font(&BODY_FONT).unwrap();
        let reader_body = reader_fonts.register(&BODY_FONT).unwrap();

        assert_eq!(runtime_body, reader_body);

        let runtime_heading = runtime.register_font(&HEADING_FONT).unwrap();
        let reader_heading = reader_fonts.register(&HEADING_FONT).unwrap();

        assert_eq!(runtime_heading, reader_heading);

        // FONT_6X10 is useful for the existing application simulator but is too small
        // for a full-screen book preview. Use the readable 10x20 face for both reader
        // body and headings when no TTF was supplied.
        (reader_heading, reader_heading)
    };

    if let Some(epub_path) = args.epub.as_deref() {
        let viewport = inkpaper_reader::Viewport::new(DISPLAY_WIDTH, DISPLAY_HEIGHT)
            .expect("simulator display dimensions are non-zero");

        let preview = load_reader_preview(
            epub_path,
            reader_fonts,
            reader_body_font,
            reader_heading_font,
            viewport,
        )
        .unwrap_or_else(|error| {
            panic!(
                "failed to open EPUB preview {}: {error:?}",
                epub_path.display(),
            )
        });

        eprintln!(
            "showing reader spine={} page=1/{}",
            preview.spine().get(),
            preview.page_count(),
        );

        runtime.create_root(move |_| preview).unwrap();

        let mut display = SimulatorDisplay::<Rgb888>::new(DISPLAY_SIZE_EG);

        rebuild_ui(&mut runtime, &mut display);

        let output_settings = OutputSettingsBuilder::new().scale(1).build();

        let file_name = epub_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("EPUB");

        let title = format!("InkPaper EPUB — {file_name}");

        let mut window = Window::new(&title, &output_settings);

        'reader: loop {
            window.update(&display);

            for event in window.events() {
                match event {
                    SimulatorEvent::Quit => break 'reader,
                    SimulatorEvent::KeyDown {
                        keycode: Keycode::Escape,
                        ..
                    } => break 'reader,
                    _ => {}
                }
            }
        }

        eprintln!(
            "glyph cache at exit: {} / {} bytes",
            runtime.glyph_cache_used_bytes(),
            runtime.glyph_cache_capacity_bytes(),
        );

        return;
    }

    let cover_image = args.cover.as_deref().map(|path| {
        HostImage::open(path)
            .unwrap_or_else(|error| panic!("failed to load cover {}: {error}", path.display()))
    });
    let cover_source = cover_image.as_ref().map(|cover| {
        runtime
            .register_image(cover)
            .expect("simulator cover image must fit registry")
    });
    if let (Some(path), Some(source)) = (args.cover.as_deref(), cover_source) {
        eprintln!(
            "cover image: {} ({}x{})",
            path.display(),
            source.size().width.get(),
            source.size().height.get(),
        );
    }

    let model = demo_model();
    let mut app_state = InkPaperApp::new(model);
    if let Some(cover) = cover_source {
        app_state = app_state.with_current_cover(cover);
    }
    let app = runtime.create_root(move |_| app_state).unwrap();

    let mut display = SimulatorDisplay::<Rgb888>::new(DISPLAY_SIZE_EG);

    rebuild_ui(&mut runtime, &mut display);

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

        update_ui(&mut runtime, &mut display);
    }

    eprintln!(
        "glyph cache at exit: {} / {} bytes",
        runtime.glyph_cache_used_bytes(),
        runtime.glyph_cache_capacity_bytes(),
    );
}
