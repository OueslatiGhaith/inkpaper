use super::*;
use crate::{BODY_FONT, load_requested_chapter};
use inkpaper_app::{
    AppEvent, AppModel, BookSummary, Button, ButtonEdge, ButtonEvent, InkPaperApp, InputEvent,
    PlatformAction, Route, TouchEvent, TouchPosition, theme::Theme,
};
use inkpaper_epub::{FontStyle, FontWeight};
use inkpaper_ui::{Entity, Runtime, Size, px};
use std::{
    cell::Cell,
    fs,
    path::PathBuf,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};

#[test]
fn reader_resources_select_body_and_heading_fonts() {
    let body = FontId::new(0);
    let heading = FontId::new(1);
    let resources = SimulatorReaderResources::new(body, heading);
    let paragraph = ReaderTextStyle::new(
        18,
        BlockKind::Paragraph,
        FontWeight::Normal,
        FontStyle::Normal,
    );
    let heading_style = ReaderTextStyle::new(
        18,
        BlockKind::Heading(2),
        FontWeight::Bold,
        FontStyle::Normal,
    );

    assert_eq!(resources.font_for(paragraph), body);
    assert_eq!(resources.font_for(heading_style), heading);
}

#[test]
fn reader_decodes_only_supported_image_formats() {
    assert!(is_supported_reader_image("image/png"));
    assert!(is_supported_reader_image("image/jpeg"));
    assert!(is_supported_reader_image("image/jpg"));
    assert!(!is_supported_reader_image("image/gif"));
    assert!(!is_supported_reader_image("image/svg+xml"));
}

type TestRuntime = Runtime<16_384, 4, 4_096, 32, 128, 2_048, 32, 2_048, 4>;

struct TestBook(PathBuf);

impl TestBook {
    fn new(broken_second: bool) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);

        let path = std::env::temp_dir().join(format!(
            "inkpaper-chapters-{}-{}.epub",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));

        let container = br#"<container><rootfiles><rootfile full-path="book.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#;

        let package = br#"<package version="3.0" xmlns="http://www.idpf.org/2007/opf">
<metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Test</dc:title></metadata>
<manifest>
<item id="a" href="a.xhtml" media-type="application/xhtml+xml"/>
<item id="skip" href="skip.xhtml" media-type="application/xhtml+xml"/>
<item id="empty" href="empty.xhtml" media-type="application/xhtml+xml"/>
<item id="b" href="b.xhtml" media-type="application/xhtml+xml"/>
<item id="other" href="other.bin" media-type="application/octet-stream"/>
<item id="c" href="c.xhtml" media-type="application/xhtml+xml"/>
</manifest><spine><itemref idref="a"/><itemref idref="skip" linear="no"/>
<itemref idref="empty"/><itemref idref="b"/><itemref idref="other"/><itemref idref="c"/>
</spine></package>"#;

        let a = format!("<html><body><p>{}</p></body></html>", "alpha ".repeat(100));
        let b = format!("<html><body><p>{}</p></body></html>", "beta ".repeat(100));
        let c = format!("<html><body><p>{}</p></body></html>", "gamma ".repeat(100));

        let mut entries: Vec<(&str, &[u8])> = vec![
            ("mimetype", b"application/epub+zip"),
            ("META-INF/container.xml", container),
            ("book.opf", package),
            ("a.xhtml", a.as_bytes()),
            ("skip.xhtml", b"<html><body><p>skip</p></body></html>"),
            ("empty.xhtml", b"<html><body></body></html>"),
            ("other.bin", b"ignored"),
            ("c.xhtml", c.as_bytes()),
        ];

        if !broken_second {
            entries.push(("b.xhtml", b.as_bytes()));
        }

        fs::write(&path, stored_zip(&entries)).unwrap();

        Self(path)
    }

    fn open(&self) -> HostReader<1> {
        let mut fonts = FontRegistry::default();
        let font = fonts.register(&BODY_FONT).unwrap();

        HostReader::open(&self.0, fonts, font, font, Viewport::new(120, 60).unwrap()).unwrap()
    }
}

impl Drop for TestBook {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn stored_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut output = Vec::new();
    let mut central = Vec::new();

    for (name, data) in entries {
        let offset = output.len() as u32;
        let size = data.len() as u32;

        let crc = !data.iter().fold(!0u32, |mut crc, byte| {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                crc = (crc >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(crc & 1));
            }
            crc
        });

        output.extend_from_slice(&0x0403_4b50u32.to_le_bytes());

        for value in [20u16, 0x0800, 0, 0, 0] {
            output.extend_from_slice(&value.to_le_bytes());
        }
        for value in [crc, size, size] {
            output.extend_from_slice(&value.to_le_bytes());
        }
        for value in [name.len() as u16, 0] {
            output.extend_from_slice(&value.to_le_bytes());
        }

        output.extend_from_slice(name.as_bytes());
        output.extend_from_slice(data);

        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());

        for value in [20u16, 20, 0x0800, 0, 0, 0] {
            central.extend_from_slice(&value.to_le_bytes());
        }
        for value in [crc, size, size] {
            central.extend_from_slice(&value.to_le_bytes());
        }
        for value in [name.len() as u16, 0, 0, 0, 0] {
            central.extend_from_slice(&value.to_le_bytes());
        }
        for value in [0u32, offset] {
            central.extend_from_slice(&value.to_le_bytes());
        }

        central.extend_from_slice(name.as_bytes());
    }

    let offset = output.len() as u32;
    let size = central.len() as u32;

    output.extend_from_slice(&central);
    output.extend_from_slice(&0x0605_4b50u32.to_le_bytes());

    for value in [0u16, 0, entries.len() as u16, entries.len() as u16] {
        output.extend_from_slice(&value.to_le_bytes());
    }
    for value in [size, offset] {
        output.extend_from_slice(&value.to_le_bytes());
    }

    output.extend_from_slice(&0u16.to_le_bytes());

    output
}

fn app_with(session: ReaderSession) -> (TestRuntime, Entity<InkPaperApp>) {
    let mut runtime = TestRuntime::default();
    runtime.set_global(Theme::EINK).unwrap();

    let book = BookSummary::try_new("Test", "Author", 0).unwrap();

    let app = runtime
        .create_root(|_| InkPaperApp::new(AppModel::new(80, book)).with_reader(session))
        .unwrap();

    (runtime, app)
}

fn press(runtime: &mut TestRuntime, app: Entity<InkPaperApp>, button: Button) -> PlatformAction {
    InkPaperApp::handle_event(
        runtime,
        app,
        AppEvent::Input(InputEvent::Button(ButtonEvent::new(
            button,
            ButtonEdge::Pressed,
        ))),
    )
}

fn position(runtime: &TestRuntime, app: Entity<InkPaperApp>) -> (u32, usize, usize) {
    runtime
        .update(app, |app, _| {
            let reader = app.reader().unwrap();
            (
                reader.spine().get(),
                reader.page_index(),
                reader.page_count(),
            )
        })
        .unwrap()
}

fn turn_to_boundary(
    runtime: &mut TestRuntime,
    app: Entity<InkPaperApp>,
    button: Button,
) -> ChapterRequest {
    for _ in 0..=position(runtime, app).2 {
        if let PlatformAction::LoadReaderChapter(request) = press(runtime, app, button) {
            return request;
        }
    }

    panic!("expected chapter boundary");
}

#[test]
fn app_reads_three_chapters_in_both_directions_and_resumes_after_home() {
    let book = TestBook::new(false);
    let mut host = book.open();

    let (session, _) = host.load_first().unwrap().into_app_session(&[]).unwrap();

    assert!(session.page_count() > 1);
    assert!(
        session
            .current_page()
            .items()
            .iter()
            .any(|item| matches!(item, PageItem::Text(text) if text.text().contains("alpha")))
    );

    let (mut runtime, app) = app_with(session);

    for expected in [3, 5] {
        let request = turn_to_boundary(&mut runtime, app, Button::Next);

        assert_eq!(press(&mut runtime, app, Button::Next), PlatformAction::None);

        load_requested_chapter(&mut runtime, app, Some(&mut host), request, &[], &[]);

        assert_eq!(position(&runtime, app).0, expected);
        assert_eq!(position(&runtime, app).1, 0);
    }

    let request = turn_to_boundary(&mut runtime, app, Button::Next);
    let end = position(&runtime, app);

    load_requested_chapter(&mut runtime, app, Some(&mut host), request, &[], &[]);

    assert_eq!(position(&runtime, app), end);

    for expected in [3, 0] {
        let request = turn_to_boundary(&mut runtime, app, Button::Previous);
        load_requested_chapter(&mut runtime, app, Some(&mut host), request, &[], &[]);

        let (spine, page, count) = position(&runtime, app);

        assert_eq!(spine, expected);
        assert_eq!(page, count - 1);
    }

    let before = position(&runtime, app);

    InkPaperApp::handle_event(
        &mut runtime,
        app,
        AppEvent::Input(InputEvent::Touch(TouchEvent::HomeTap)),
    );

    runtime
        .update(app, |app, _| assert_eq!(app.route(), Route::Home))
        .unwrap();
    runtime.rebuild().unwrap();
    runtime
        .layout_with_measurer(Size::new(px(480), px(800)), &TestMeasurer)
        .unwrap();

    // continue reading is the first focusable Home control with a session installed.
    press(&mut runtime, app, Button::Next);
    press(&mut runtime, app, Button::Activate);

    InkPaperApp::handle_event(
        &mut runtime,
        app,
        AppEvent::Input(InputEvent::Button(ButtonEvent::new(
            Button::Activate,
            ButtonEdge::Released,
        ))),
    );

    runtime
        .update(app, |app, _| assert_eq!(app.route(), Route::Reader))
        .unwrap();

    assert_eq!(position(&runtime, app), before);

    let request = turn_to_boundary(&mut runtime, app, Button::Previous);
    load_requested_chapter(&mut runtime, app, Some(&mut host), request, &[], &[]);

    assert_eq!(position(&runtime, app).1, 0);
    assert_eq!(press(&mut runtime, app, Button::Next), PlatformAction::None);
    assert_eq!(position(&runtime, app).1, 1);
}

#[test]
fn failed_chapter_load_keeps_current_page_and_allows_retry_or_reverse() {
    let book = TestBook::new(true);
    let mut host = book.open();

    let (session, _) = host.load_first().unwrap().into_app_session(&[]).unwrap();
    let (mut runtime, app) = app_with(session);

    for _ in 0..2 {
        let request = turn_to_boundary(&mut runtime, app, Button::Next);
        let before = position(&runtime, app);

        load_requested_chapter(&mut runtime, app, Some(&mut host), request, &[], &[]);

        assert_eq!(position(&runtime, app), before);
    }

    let before = position(&runtime, app);

    assert_eq!(
        press(&mut runtime, app, Button::Previous),
        PlatformAction::None
    );
    assert_eq!(position(&runtime, app).1, before.1 - 1);
}

struct DropResources(Rc<Cell<usize>>);

impl ReaderPageResources for DropResources {}

impl Drop for DropResources {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

#[test]
fn replacing_session_drops_old_resources_and_home_rejects_pending_completion() {
    let book = TestBook::new(false);
    let mut host = book.open();

    let prepared = host.load_first().unwrap();
    let drops = Rc::new(Cell::new(0));
    let session = ReaderSession::new(
        prepared.pagination,
        prepared.viewport,
        Box::new(DropResources(drops.clone())),
    )
    .unwrap();

    let (mut runtime, app) = app_with(session);
    let request = turn_to_boundary(&mut runtime, app, Button::Next);

    load_requested_chapter(&mut runtime, app, Some(&mut host), request, &[], &[]);

    assert_eq!(drops.get(), 1);

    let request = turn_to_boundary(&mut runtime, app, Button::Next);
    let before = position(&runtime, app);

    InkPaperApp::handle_event(
        &mut runtime,
        app,
        AppEvent::Input(InputEvent::Touch(TouchEvent::HomeTap)),
    );

    load_requested_chapter(&mut runtime, app, Some(&mut host), request, &[], &[]);

    assert_eq!(position(&runtime, app), before);

    runtime
        .update(app, |app, cx| app.navigate(Route::Reader, cx))
        .unwrap();

    assert!(matches!(
        press(&mut runtime, app, Button::Next),
        PlatformAction::LoadReaderChapter(_)
    ));
}

#[test]
fn owned_pages_render_and_touch_requests_use_the_same_boundary_path() {
    let book = TestBook::new(false);
    let mut host = book.open();

    let (session, _) = host.load_first().unwrap().into_app_session(&[]).unwrap();
    let (mut runtime, app) = app_with(session);

    runtime.set_global(Theme::EINK).unwrap();
    runtime.rebuild().unwrap();
    runtime
        .layout_with_measurer(Size::new(px(120), px(60)), &TestMeasurer)
        .unwrap();

    let right = TouchPosition::new(100, 30);
    let left = TouchPosition::new(10, 30);

    for _ in 0..position(&runtime, app).2 - 1 {
        assert_eq!(press(&mut runtime, app, Button::Next), PlatformAction::None);
    }

    assert_eq!(
        InkPaperApp::handle_event(
            &mut runtime,
            app,
            AppEvent::Input(InputEvent::Touch(TouchEvent::Down(right)))
        ),
        PlatformAction::None
    );

    let action = InkPaperApp::handle_event(
        &mut runtime,
        app,
        AppEvent::Input(InputEvent::Touch(TouchEvent::Up(right))),
    );

    let PlatformAction::LoadReaderChapter(request) = action else {
        panic!("expected touch request")
    };

    load_requested_chapter(&mut runtime, app, Some(&mut host), request, &[], &[]);

    assert_eq!(position(&runtime, app).0, 3);

    runtime.rebuild().unwrap();
    runtime
        .layout_with_measurer(Size::new(px(120), px(60)), &TestMeasurer)
        .unwrap();

    InkPaperApp::handle_event(
        &mut runtime,
        app,
        AppEvent::Input(InputEvent::Touch(TouchEvent::Down(left))),
    );

    let action = InkPaperApp::handle_event(
        &mut runtime,
        app,
        AppEvent::Input(InputEvent::Touch(TouchEvent::Up(left))),
    );

    let PlatformAction::LoadReaderChapter(request) = action else {
        panic!("expected touch request")
    };

    assert_eq!(request.direction, ChapterDirection::Previous);
}

struct TestMeasurer;

impl inkpaper_ui::TextMeasurer for TestMeasurer {
    fn measure_text(&self, text: &str, _: inkpaper_ui::ResolvedTextStyle, _: Size) -> Size {
        Size::new(px(text.len() as i32 * 6), px(10))
    }
}
