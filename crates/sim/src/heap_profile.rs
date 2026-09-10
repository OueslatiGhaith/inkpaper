use std::path::Path;

use embedded_graphics::pixelcolor::Rgb888;
use embedded_graphics_simulator::SimulatorDisplay;
use inkpaper_app::{
    AppEvent, AppModel, Button as AppButton, ButtonEdge as AppButtonEdge, InkPaperApp,
    InputEvent as AppInputEvent, PlatformAction, Route, TouchEvent as AppTouchEvent, theme::Theme,
};
use inkpaper_epub::SpineIndex;
use inkpaper_reader::Viewport;
use inkpaper_ui::{FontFace, FontRegistry, prelude::*};

use crate::{
    BODY_FONT, DISPLAY_HEIGHT, DISPLAY_SIZE_EG, DISPLAY_WIDTH, HEADING_FONT, READER_IMAGE_CAPACITY,
    args::SimulatorArgs,
    button_event, handle_app_event, heap,
    host_image::HostImageSlot,
    install_reader_images, load_initial_reader, load_requested_chapter, load_requested_page,
    reader_demo::HostReader,
    rebuild_ui,
    runtime::{SimulatorRuntime, new_runtime},
    runtime_font_path, update_ui,
};

pub fn run(args: SimulatorArgs) {
    let path = args.epub.as_deref().expect("profiling requires --epub");
    let profiler = dhat::Profiler::builder()
        .file_name(&args.profile_output)
        .build();

    eprintln!(
        "heap profile: UI alloc={} turns={} cycles={}; no progress files are read or written",
        cfg!(feature = "alloc"),
        args.profile_turns,
        args.profile_cycles,
    );

    let startup = heap::report("startup", 0);
    let font = runtime_font_path(args.font.as_deref()).map(|font| font as &'static dyn FontFace);
    let font_baseline = heap::report("font-ready", startup);

    for cycle in 1..=args.profile_cycles {
        eprintln!("profile cycle={cycle}");
        let baseline = heap::report("cycle-start", font_baseline);
        run_cycle(path, font, args.profile_turns, baseline);
        heap::report("cycle-released", baseline);
    }

    heap::report("finished", startup);
    drop(profiler);
}

fn run_cycle(path: &Path, font: Option<&'static dyn FontFace>, turns: usize, baseline: usize) {
    let slots: [HostImageSlot; READER_IMAGE_CAPACITY] =
        std::array::from_fn(|_| HostImageSlot::default());

    let mut runtime = new_runtime();
    runtime.set_global(Theme::EINK).unwrap();

    let mut fonts = FontRegistry::<2>::default();
    let (body, heading) = if let Some(font) = font {
        let id = runtime.register_font(font).unwrap();
        assert_eq!(id, fonts.register(font).unwrap());
        (id, id)
    } else {
        let body = runtime.register_font(&BODY_FONT).unwrap();
        assert_eq!(body, fonts.register(&BODY_FONT).unwrap());

        let heading = runtime.register_font(&HEADING_FONT).unwrap();
        assert_eq!(heading, fonts.register(&HEADING_FONT).unwrap());
        (heading, heading)
    };
    heap::report("runtime-ready", baseline);

    let mut host = HostReader::open(
        path,
        fonts,
        body,
        heading,
        Viewport::new(DISPLAY_WIDTH, DISPLAY_HEIGHT).unwrap(),
    )
    .unwrap_or_else(|error| panic!("failed to open {}: {error:?}", path.display()));
    heap::report("epub-open", baseline);

    let ids: Vec<_> = slots
        .iter()
        .map(|slot| runtime.register_image(slot).unwrap().id())
        .collect();

    let (session, images) = load_initial_reader(&mut host, None, &ids)
        .unwrap_or_else(|error| panic!("failed to prepare reader: {error:?}"));
    install_reader_images(&slots, images);

    let model = AppModel::new(73, host.book_summary());
    let app = runtime
        .create_root(move |_| InkPaperApp::new(model).with_reader(session))
        .unwrap();
    heap::report("reader-prepared", baseline);

    let mut display = SimulatorDisplay::<Rgb888>::new(DISPLAY_SIZE_EG);
    heap::report("display-ready", baseline);

    rebuild_ui(runtime.as_mut(), &mut display);
    report_state(runtime.as_ref(), app);
    heap::report("first-page-painted", baseline);

    for turn in 1..=turns {
        let before = position(runtime.as_ref(), app);

        for edge in [AppButtonEdge::Pressed, AppButtonEdge::Released] {
            dispatch(
                runtime.as_mut(),
                app,
                &mut host,
                &slots,
                &ids,
                button_event(AppButton::Next, edge),
            );

            update_ui(runtime.as_mut(), &mut display);
        }

        eprintln!("profile turn={turn}");
        report_state(runtime.as_ref(), app);
        heap::report("page-painted", baseline);

        if position(runtime.as_ref(), app) == before {
            eprintln!(
                "profile: stopped forward turns because the reading position did not advance"
            );
            break;
        }
    }

    let before_home = position(runtime.as_ref(), app);
    dispatch(
        runtime.as_mut(),
        app,
        &mut host,
        &slots,
        &ids,
        AppEvent::Input(AppInputEvent::Touch(AppTouchEvent::HomeTap)),
    );
    update_ui(runtime.as_mut(), &mut display);

    assert_eq!(position(runtime.as_ref(), app).0, Route::Home);
    report_state(runtime.as_ref(), app);
    heap::report("home-painted-session-retained", baseline);

    // continue reading is the first focusable Home control with a session installed.
    for button in [AppButton::Next, AppButton::Activate] {
        for edge in [AppButtonEdge::Pressed, AppButtonEdge::Released] {
            dispatch(
                runtime.as_mut(),
                app,
                &mut host,
                &slots,
                &ids,
                button_event(button, edge),
            );
            update_ui(runtime.as_mut(), &mut display);
        }
    }

    assert_eq!(position(runtime.as_ref(), app), before_home);
    report_state(runtime.as_ref(), app);
    heap::report("reader-reopened", baseline);

    // the registry must be destroyed before its borrowed image slots.
    drop(runtime);
}

fn dispatch<const FONTS: usize>(
    runtime: &mut impl RuntimeApi,
    app: Entity<InkPaperApp>,
    host: &mut HostReader<FONTS>,
    slots: &[HostImageSlot],
    ids: &[ImageId],
    event: AppEvent,
) {
    match handle_app_event(runtime, app, event) {
        PlatformAction::None => {}
        PlatformAction::LoadReaderChapter(request) => {
            load_requested_chapter(runtime, app, Some(host), request, slots, ids);
        }
        PlatformAction::LoadReaderPage(request) => {
            load_requested_page(runtime, app, Some(host), request, slots, ids);
        }
        PlatformAction::Suspend => panic!("unexpected suspend during heap profile"),
    }
}

fn position(runtime: &impl RuntimeApi, app: Entity<InkPaperApp>) -> (Route, SpineIndex, usize) {
    runtime
        .update(app, |app, _| {
            let reader = app.reader().expect("profile reader must remain installed");
            (app.route(), reader.spine(), reader.page_number())
        })
        .unwrap()
}

fn report_state(runtime: &SimulatorRuntime<'_>, app: Entity<InkPaperApp>) {
    eprintln!(
        "frame: nodes={} text_bytes={}",
        runtime.frame_node_count(),
        runtime.frame_text_bytes_used()
    );

    runtime
        .update(app, |app, _| {
            let reader = app.reader().expect("profile reader must remain installed");
            eprintln!(
                "profile: route={:?} spine={} page={}/{} notice={:?}",
                app.route(),
                reader.spine().get(),
                reader.page_number(),
                reader.page_count(),
                reader.notice(),
            );
        })
        .unwrap();
}
