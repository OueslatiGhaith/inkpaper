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
    reader::{ChapterRequest, PageRequest, ReaderSession},
    theme::Theme,
};
use inkpaper_reader::Viewport;
use inkpaper_ui::{
    FontData, FontFace, FontRegistry, TtfFont,
    backend::{CoverageMode, EmbeddedGraphicsPainter, MonoFontFace},
    prelude::*,
};
use static_cell::StaticCell;

use crate::{
    args::SimulatorArgs,
    host_image::{HostImage, HostImageSlot},
    reader_demo::{DecodedReaderImage, HostReader, ReaderLoadError},
    reader_progress::ReaderProgress,
};

mod args;
mod host_epub;
mod host_image;
mod reader_demo;
mod reader_progress;

const DISPLAY_WIDTH: u32 = 480;
const DISPLAY_HEIGHT: u32 = 800;

const DISPLAY_SIZE_EG: EgSize = EgSize::new(DISPLAY_WIDTH, DISPLAY_HEIGHT);
const DISPLAY_SIZE: Size = Size::new(px(DISPLAY_WIDTH as i32), px(DISPLAY_HEIGHT as i32));

const RUNTIME_FONT_STORAGE_BYTES: usize = 5 * 1024 * 1024;

static RUNTIME_FONT_BYTES: StaticCell<Box<[u8; RUNTIME_FONT_STORAGE_BYTES]>> = StaticCell::new();
static RUNTIME_FONT: StaticCell<TtfFont> = StaticCell::new();

static BODY_FONT: MonoFontFace<'static> = MonoFontFace::ascii(&FONT_6X10);
static HEADING_FONT: MonoFontFace<'static> = MonoFontFace::ascii(&FONT_10X20);

const READER_IMAGE_CAPACITY: usize = 16;

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

fn install_reader_images(slots: &[HostImageSlot], images: Vec<DecodedReaderImage>) {
    assert!(images.len() <= slots.len());

    let mut images = images.into_iter();
    for slot in slots {
        slot.replace(images.next().map(DecodedReaderImage::into_image));
    }
}

fn load_requested_chapter<const FONTS: usize>(
    runtime: &mut impl RuntimeApi,
    app: Entity<InkPaperApp>,
    host: Option<&mut HostReader<FONTS>>,
    request: ChapterRequest,
    slots: &[HostImageSlot],
    ids: &[ImageId],
) {
    let prepared = match host {
        Some(host) => host.load_adjacent(request).and_then(|prepared| {
            prepared
                .map(|prepared| prepared.into_app_session(ids))
                .transpose()
        }),
        None => Ok(None),
    };

    match prepared {
        Ok(Some((session, images))) => {
            let spine = session.spine();
            runtime.cancel_activation();

            let accepted = runtime
                .update(app, |app, cx| {
                    app.complete_reader_chapter(request, Some(session), cx)
                })
                .expect("InkPaper application entity must remain alive");

            if accepted {
                install_reader_images(slots, images);
                eprintln!("reader: entered spine={}", spine.get());
            }
        }
        result => {
            if let Err(error) = result {
                eprintln!("reader chapter load failed: {error:?}");
            }

            runtime
                .update(app, |app, cx| {
                    app.complete_reader_chapter(request, None, cx);
                })
                .expect("InkPaper application entity must remain alive");
        }
    }
}

fn load_requested_page<const FONTS: usize>(
    runtime: &mut impl RuntimeApi,
    app: Entity<InkPaperApp>,
    host: Option<&mut HostReader<FONTS>>,
    request: PageRequest,
    slots: &[HostImageSlot],
    ids: &[ImageId],
) {
    let prepared = runtime
        .update(app, |app, _| {
            let Some(page) = app
                .reader()
                .and_then(|reader| reader.page_for_request(request))
            else {
                return Ok(None);
            };
            match host {
                Some(host) => host.load_page(page, ids).map(Some),
                None => Ok(None),
            }
        })
        .expect("InkPaper application entity must remain alive");

    match prepared {
        Ok(Some((resources, images))) => {
            runtime.cancel_activation();

            let accepted = runtime
                .update(app, |app, cx| {
                    app.complete_reader_page(request, Some(resources), cx)
                })
                .expect("InkPaper application entity must remain alive");

            if accepted {
                install_reader_images(slots, images);
            }
        }
        result => {
            if let Err(error) = result {
                eprintln!("reader page image load failed: {error:?}");
            }

            runtime
                .update(app, |app, cx| {
                    app.complete_reader_page(request, None, cx);
                })
                .expect("InkPaper application entity must remain alive");
        }
    }
}

fn load_initial_reader<const FONTS: usize>(
    host: &mut HostReader<FONTS>,
    progress: Option<&mut ReaderProgress>,
    ids: &[ImageId],
) -> Result<(ReaderSession, Vec<DecodedReaderImage>), ReaderLoadError> {
    let saved = match progress.as_deref().map(ReaderProgress::load) {
        Some(Ok(position)) => position,
        Some(Err(error)) => {
            eprintln!("ignoring saved reader progress: {error}");
            None
        }
        None => None,
    };

    let restored = saved.and_then(|position| {
        match host
            .load_position(position)
            .and_then(|prepared| prepared.into_app_session(ids))
        {
            Ok(prepared) => {
                eprintln!(
                    "reader resumed: spine={} page={}",
                    prepared.0.spine().get(),
                    prepared.0.page_number()
                );
                Some(prepared)
            }
            Err(error) => {
                eprintln!("could not restore reader position; opening the beginning: {error:?}");
                None
            }
        }
    });

    let (session, images) = match restored {
        Some(prepared) => prepared,
        None => host.load_first()?.into_app_session(ids)?,
    };

    if let Some(progress) = progress {
        progress.start_at(session.current_page().position());
    }

    Ok((session, images))
}

fn checkpoint_reader(
    runtime: &impl RuntimeApi,
    app: Entity<InkPaperApp>,
    progress: Option<&mut ReaderProgress>,
) {
    let Some(progress) = progress else { return };

    let position = runtime
        .update(app, |app, _| {
            app.reader().map(|reader| reader.current_page().position())
        })
        .expect("InkPaper application entity must remain alive");

    if let Some(position) = position
        && let Err(error) = progress.checkpoint(position)
    {
        eprintln!(
            "could not save reader progress to {}: {error}",
            progress.path().display()
        );
    }
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
        Keycode::H | Keycode::Home => {
            return (edge == AppButtonEdge::Pressed).then_some(AppEvent::Input(
                AppInputEvent::Touch(AppTouchEvent::HomeTap),
            ));
        }
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

    let reader_slots: [HostImageSlot; READER_IMAGE_CAPACITY] =
        std::array::from_fn(|_| HostImageSlot::default());

    let mut runtime = Box::new(
        RuntimeBuilder::default()
            .entities::<16_384, 32>()
            .callbacks::<8_192, 64>()
            .frame::<2_048, 32_768>()
            .element_states::<256>()
            .globals::<2_048, 8>()
            .render_resources::<2, 128, { 16 * 1024 }, READER_IMAGE_CAPACITY>()
            .build(),
    );
    runtime.set_global(Theme::EINK).unwrap();

    let runtime_font = runtime_font_path(args.font.as_deref());
    let runtime_font = runtime_font.map(|font| font as &'static dyn FontFace);

    // keep font IDs identical between pagination and painting.
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

        // use the readable 10x20 face for reader text when no TTF was supplied.
        (reader_heading, reader_heading)
    };

    let mut host_reader = args.epub.as_deref().map(|path| {
        HostReader::open(
            path,
            reader_fonts,
            reader_body_font,
            reader_heading_font,
            Viewport::new(DISPLAY_WIDTH, DISPLAY_HEIGHT).unwrap(),
        )
        .unwrap_or_else(|error| panic!("failed to open EPUB {}: {error:?}", path.display()))
    });

    let mut reader_progress =
        args.epub.as_deref().filter(|_| !args.no_resume).and_then(
            |path| match ReaderProgress::open(path) {
                Ok(progress) => {
                    eprintln!("reader progress: {}", progress.path().display());
                    Some(progress)
                }
                Err(error) => {
                    eprintln!("reader progress unavailable: {error}");
                    None
                }
            },
        );

    let mut reader_image_ids = Vec::new();
    let reader_session = host_reader.as_mut().map(|host| {
        for slot in &reader_slots {
            reader_image_ids.push(runtime.register_image(slot).unwrap().id());
        }

        let (session, images) =
            load_initial_reader(host, reader_progress.as_mut(), &reader_image_ids)
                .unwrap_or_else(|error| panic!("failed to prepare EPUB: {error:?}"));

        install_reader_images(&reader_slots, images);

        eprintln!(
            "reader installed: spine={} pages={}",
            session.spine().get(),
            session.page_count()
        );

        session
    });

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

    let model = host_reader
        .as_ref()
        .map_or_else(demo_model, |host| AppModel::new(73, host.book_summary()));
    let mut app_state = InkPaperApp::new(model);

    if let Some(reader) = reader_session {
        app_state = app_state.with_reader(reader);
    }
    if let Some(cover) = cover_source {
        app_state = app_state.with_current_cover(cover);
    }
    let app = runtime.create_root(move |_| app_state).unwrap();

    let mut display = SimulatorDisplay::<Rgb888>::new(DISPLAY_SIZE_EG);

    rebuild_ui(runtime.as_mut(), &mut display);

    let output_settings = OutputSettingsBuilder::new().scale(1).build();

    let window_title = args
        .epub
        .as_deref()
        .and_then(|path| path.file_name())
        .and_then(|name| name.to_str())
        .map(|name| format!("InkPaper — {name}"))
        .unwrap_or_else(|| String::from("InkPaper X4 Pro"));

    let mut window = Window::new(&window_title, &output_settings);

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
                        runtime.as_mut(),
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
                        runtime.as_mut(),
                        app,
                        AppEvent::Input(AppInputEvent::Touch(AppTouchEvent::Up(mouse_position))),
                    )
                }
                SimulatorEvent::MouseWheel { scroll_delta, .. } => {
                    const SCROLL_STEP: i32 = 18;
                    let delta_x = -scroll_delta.x.saturating_mul(SCROLL_STEP);
                    let delta_y = -scroll_delta.y.saturating_mul(SCROLL_STEP);
                    handle_app_event(
                        runtime.as_mut(),
                        app,
                        AppEvent::Input(AppInputEvent::Scroll(AppScrollEvent::new(
                            mouse_position,
                            delta_x,
                            delta_y,
                        ))),
                    )
                }
                SimulatorEvent::KeyDown {
                    keycode: Keycode::Escape,
                    repeat: false,
                    ..
                } => handle_app_event(
                    runtime.as_mut(),
                    app,
                    AppEvent::Input(AppInputEvent::Touch(AppTouchEvent::HomeTap)),
                ),
                SimulatorEvent::KeyDown {
                    keycode,
                    repeat: false,
                    ..
                } => match key_event(keycode, AppButtonEdge::Pressed) {
                    Some(event) => handle_app_event(runtime.as_mut(), app, event),
                    None => PlatformAction::None,
                },
                SimulatorEvent::KeyUp { keycode, .. } => {
                    match key_event(keycode, AppButtonEdge::Released) {
                        Some(event) => handle_app_event(runtime.as_mut(), app, event),
                        None => PlatformAction::None,
                    }
                }
                _ => PlatformAction::None,
            };

            match action {
                PlatformAction::None => {}
                PlatformAction::LoadReaderChapter(request) => {
                    load_requested_chapter(
                        runtime.as_mut(),
                        app,
                        host_reader.as_mut(),
                        request,
                        &reader_slots,
                        &reader_image_ids,
                    );
                }
                PlatformAction::LoadReaderPage(request) => {
                    load_requested_page(
                        runtime.as_mut(),
                        app,
                        host_reader.as_mut(),
                        request,
                        &reader_slots,
                        &reader_image_ids,
                    );
                }
                PlatformAction::Suspend => {
                    // the real X4 platform performs its complete deep-sleep sequence.
                    break 'running;
                }
            }

            checkpoint_reader(runtime.as_ref(), app, reader_progress.as_mut());

            // later events must hit the newly installed page or route.
            update_ui(runtime.as_mut(), &mut display);
        }
    }

    if let Some(progress) = reader_progress.as_mut()
        && let Err(error) = progress.flush()
    {
        eprintln!("could not save reader progress on exit: {error}");
    }

    eprintln!(
        "glyph cache at exit: {} / {} bytes",
        runtime.glyph_cache_used_bytes(),
        runtime.glyph_cache_capacity_bytes(),
    );

    // destroy the registry before its borrowed image slots and cover.
    drop(runtime);
    drop(reader_slots);
    drop(cover_image);
}
