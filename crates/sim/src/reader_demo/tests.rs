use super::*;
use crate::{BODY_FONT, host_image::HostImageSlot, load_requested_chapter, load_requested_page};
use inkpaper_app::{
    AppEvent, AppModel, BookSummary, Button, ButtonEdge, ButtonEvent, InkPaperApp, InputEvent,
    PlatformAction, Route, TouchEvent, TouchPosition,
    reader::{ChapterLoadOutcome, PageLoadOutcome, ReaderNotice},
    theme::Theme,
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

fn png(width: u32, height: u32, color: [u8; 3]) -> Vec<u8> {
    let pixels = image::RgbImage::from_pixel(width, height, image::Rgb(color));
    let mut output = std::io::Cursor::new(Vec::new());

    image::DynamicImage::ImageRgb8(pixels)
        .write_to(&mut output, image::ImageFormat::Png)
        .unwrap();

    output.into_inner()
}

fn illustrated_book(body: &str, next: &str, images: &[(&str, Vec<u8>)]) -> TestBook {
    static NEXT: AtomicU64 = AtomicU64::new(0);

    let path = std::env::temp_dir().join(format!(
        "inkpaper-page-images-{}-{}.epub",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed),
    ));

    let container = br#"<container><rootfiles><rootfile full-path="book.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#;

    let mut manifest = String::new();

    for (index, (name, _)) in images.iter().enumerate() {
        manifest.push_str(&format!(
            r#"<item id="image{index}" href="{name}" media-type="image/png"/>"#
        ));
    }

    let package = format!(
        r#"<package version="3.0" xmlns="http://www.idpf.org/2007/opf">
<metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Images</dc:title></metadata>
<manifest><item id="a" href="a.xhtml" media-type="application/xhtml+xml"/>
<item id="b" href="b.xhtml" media-type="application/xhtml+xml"/>{manifest}</manifest>
<spine><itemref idref="a"/><itemref idref="b"/></spine></package>"#
    );

    let a = format!("<html><body>{body}</body></html>");
    let b = format!("<html><body>{next}</body></html>");

    let mut entries: Vec<(&str, &[u8])> = vec![
        ("mimetype", b"application/epub+zip"),
        ("META-INF/container.xml", container),
        ("book.opf", package.as_bytes()),
        ("a.xhtml", a.as_bytes()),
        ("b.xhtml", b.as_bytes()),
    ];

    entries.extend(images.iter().map(|(name, bytes)| (*name, bytes.as_slice())));

    fs::write(&path, stored_zip(&entries)).unwrap();

    TestBook(path)
}

fn page_request(
    runtime: &mut TestRuntime,
    app: Entity<InkPaperApp>,
    button: Button,
) -> inkpaper_app::reader::PageRequest {
    let before = position(runtime, app);

    let PlatformAction::LoadReaderPage(request) = press(runtime, app, button) else {
        panic!("expected page image request");
    };

    assert_eq!(position(runtime, app), before);
    assert_eq!(press(runtime, app, button), PlatformAction::None);

    request
}

fn displayed_images(
    runtime: &TestRuntime,
    app: Entity<InkPaperApp>,
) -> Vec<(ArchivePath, ImageSource)> {
    runtime
        .update(app, |app, _| {
            let reader = app.reader().unwrap();

            reader
                .current_page()
                .items()
                .iter()
                .filter_map(|item| {
                    let PageItem::Image(fragment) = item else {
                        return None;
                    };

                    Some((
                        fragment.image().path().clone(),
                        reader.resources().image_source(fragment.image()).unwrap(),
                    ))
                })
                .collect()
        })
        .unwrap()
}

#[test]
fn later_pages_and_chapters_reuse_slots_without_stale_mappings() {
    use crate::{host_image::HostImageSlot, install_reader_images, load_requested_page};

    let red = [200, 10, 20];
    let blue = [10, 20, 200];

    let book = illustrated_book(
        r#"<p>start</p><img src="red.png"/><img src="blue.png"/><img src="red.png"/><p>end</p>"#,
        r#"<img src="blue.png"/>"#,
        &[
            ("red.png", png(120, 60, red)),
            ("blue.png", png(120, 60, blue)),
        ],
    );
    let mut host = book.open();

    let slots = [HostImageSlot::default()];
    let mut registry = inkpaper_ui::ImageRegistry::<1>::default();

    let ids = [registry.register(&slots[0]).unwrap().id()];
    let (session, images) = host.load_first().unwrap().into_app_session(&ids).unwrap();

    assert_eq!(session.page_count(), 5);

    install_reader_images(&slots, images);

    let (mut runtime, app) = app_with(session);

    for (expected_page, color) in [(1, red), (2, blue), (3, red)] {
        let request = page_request(&mut runtime, app, Button::Next);
        load_requested_page(&mut runtime, app, Some(&mut host), request, &slots, &ids);

        assert_eq!(position(&runtime, app).1, expected_page);

        let sources = displayed_images(&runtime, app);

        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].1.id(), ids[0]);
        assert_eq!(
            registry.get(ids[0]).unwrap().pixel(0, 0),
            Some(inkpaper_ui::Color::rgb(color[0], color[1], color[2]))
        );
    }

    // a text page and return to its still-resident image require no preparation.
    assert_eq!(press(&mut runtime, app, Button::Next), PlatformAction::None);
    assert_eq!(position(&runtime, app).1, 4);
    assert_eq!(
        press(&mut runtime, app, Button::Previous),
        PlatformAction::None
    );
    assert_eq!(position(&runtime, app).1, 3);

    let request = page_request(&mut runtime, app, Button::Previous);
    load_requested_page(&mut runtime, app, Some(&mut host), request, &slots, &ids);

    assert_eq!(position(&runtime, app).1, 2);
    assert_eq!(
        registry.get(ids[0]).unwrap().pixel(0, 0),
        Some(inkpaper_ui::Color::rgb(10, 20, 200))
    );

    let request = page_request(&mut runtime, app, Button::Next);
    load_requested_page(&mut runtime, app, Some(&mut host), request, &slots, &ids);
    press(&mut runtime, app, Button::Next);

    let request = turn_to_boundary(&mut runtime, app, Button::Next);
    load_requested_chapter(&mut runtime, app, Some(&mut host), request, &slots, &ids);

    assert_eq!(position(&runtime, app).0, 1);
    assert_eq!(displayed_images(&runtime, app)[0].0.as_str(), "blue.png");
    assert_eq!(
        registry.get(ids[0]).unwrap().pixel(0, 0),
        Some(inkpaper_ui::Color::rgb(10, 20, 200))
    );

    let request = turn_to_boundary(&mut runtime, app, Button::Previous);
    load_requested_chapter(&mut runtime, app, Some(&mut host), request, &slots, &ids);

    assert_eq!(position(&runtime, app).1, 4);

    let request = page_request(&mut runtime, app, Button::Previous);
    load_requested_page(&mut runtime, app, Some(&mut host), request, &slots, &ids);

    assert_eq!(displayed_images(&runtime, app)[0].0.as_str(), "red.png");
    assert_eq!(registry.len(), 1);
}

#[test]
fn repeated_images_on_one_page_use_one_slot_and_decode_once() {
    let book = illustrated_book(
        r#"<img src="large.png"/><img src="small.png"/><img src="small.png"/>"#,
        "<p>end</p>",
        &[
            ("large.png", png(120, 60, [1, 2, 3])),
            ("small.png", png(2, 1, [4, 5, 6])),
        ],
    );
    let mut host = book.open();

    let slots = [HostImageSlot::default()];
    let ids = [ImageId::new(0)];

    let (session, images) = host.load_first().unwrap().into_app_session(&ids).unwrap();
    crate::install_reader_images(&slots, images);

    let (mut runtime, app) = app_with(session);

    let request = page_request(&mut runtime, app, Button::Next);

    let count = runtime
        .update(app, |app, _| {
            let page = app.reader().unwrap().page_for_request(request).unwrap();
            host.load_page(page, &ids).unwrap().1.len()
        })
        .unwrap();

    assert_eq!(count, 1);

    load_requested_page(&mut runtime, app, Some(&mut host), request, &slots, &ids);

    let images = displayed_images(&runtime, app);

    assert_eq!(images.len(), 2);
    assert_eq!(images[0].1, images[1].1);
}

#[test]
fn decode_and_capacity_failures_preserve_page_pixels_and_allow_retry() {
    use crate::{host_image::HostImageSlot, load_requested_page};

    for capacity_failure in [false, true] {
        let mut bad = png(2, 1, [4, 5, 6]);
        bad.truncate(24); // valid dimensions, incomplete pixel data.

        let body = if capacity_failure {
            r#"<img src="large.png"/><img src="small.png"/><img src="bad.png"/>"#
        } else {
            r#"<img src="large.png"/><img src="bad.png"/>"#
        };

        let book = illustrated_book(
            body,
            "<p>end</p>",
            &[
                ("large.png", png(120, 60, [1, 2, 3])),
                ("small.png", png(2, 1, [7, 8, 9])),
                ("bad.png", bad),
            ],
        );

        let mut host = book.open();

        let slots = [HostImageSlot::default()];
        let ids = [ImageId::new(0)];

        let (session, images) = host.load_first().unwrap().into_app_session(&ids).unwrap();
        crate::install_reader_images(&slots, images);

        let (mut runtime, app) = app_with(session);
        let before = displayed_images(&runtime, app);

        for _ in 0..2 {
            let request = page_request(&mut runtime, app, Button::Next);

            runtime
                .update(app, |app, _| {
                    let page = app.reader().unwrap().page_for_request(request).unwrap();
                    let error = host.load_page(page, &ids).err().unwrap();

                    if capacity_failure {
                        assert!(matches!(
                            error,
                            ReaderLoadError::TooManyImages {
                                count: 2,
                                capacity: 1
                            }
                        ));
                    } else {
                        assert!(matches!(error, ReaderLoadError::ImageDecode { .. }));
                    }
                })
                .unwrap();

            load_requested_page(&mut runtime, app, Some(&mut host), request, &slots, &ids);

            assert_eq!(position(&runtime, app).1, 0);
            assert_eq!(displayed_images(&runtime, app), before);
            assert_eq!(slots[0].pixel(0, 0), Some(inkpaper_ui::Color::rgb(1, 2, 3)));
        }
    }
}

struct ChangedFontResources(Box<dyn ReaderPageResources>);

impl ReaderPageResources for ChangedFontResources {
    fn font_for(&self, _: ReaderTextStyle) -> FontId {
        FontId::new(99)
    }

    fn image_source(&self, image: &inkpaper_reader::ChapterImage) -> Option<ImageSource> {
        self.0.image_source(image)
    }
}

#[test]
fn invalid_page_resources_and_home_cancellation_leave_current_images_intact() {
    use crate::{host_image::HostImageSlot, load_requested_page};

    let book = illustrated_book(
        r#"<img src="large.png"/><img src="small.png"/><p>end</p>"#,
        "<p>next</p>",
        &[
            ("large.png", png(120, 60, [1, 2, 3])),
            ("small.png", png(120, 60, [4, 5, 6])),
        ],
    );
    let mut host = book.open();

    let slots = [HostImageSlot::default()];
    let ids = [ImageId::new(0)];

    let (session, images) = host.load_first().unwrap().into_app_session(&ids).unwrap();
    crate::install_reader_images(&slots, images);

    let (mut runtime, app) = app_with(session);

    for wrong_font in [false, true] {
        let request = page_request(&mut runtime, app, Button::Next);

        runtime
            .update(app, |app, cx| {
                let resources: Box<dyn ReaderPageResources> = if wrong_font {
                    let page = app.reader().unwrap().page_for_request(request).unwrap();
                    Box::new(ChangedFontResources(host.load_page(page, &ids).unwrap().0))
                } else {
                    Box::new(())
                };

                assert!(!app.complete_reader_page(request, PageLoadOutcome::Ready(resources), cx));
            })
            .unwrap();

        assert_eq!(position(&runtime, app).1, 0);
    }

    let request = page_request(&mut runtime, app, Button::Next);

    InkPaperApp::handle_event(
        &mut runtime,
        app,
        AppEvent::Input(InputEvent::Touch(TouchEvent::HomeTap)),
    );
    load_requested_page(&mut runtime, app, Some(&mut host), request, &slots, &ids);

    assert_eq!(position(&runtime, app).1, 0);
    assert_eq!(slots[0].pixel(0, 0), Some(inkpaper_ui::Color::rgb(1, 2, 3)));

    runtime
        .update(app, |app, cx| app.navigate(Route::Reader, cx))
        .unwrap();

    let request = page_request(&mut runtime, app, Button::Next);
    load_requested_page(&mut runtime, app, Some(&mut host), request, &slots, &ids);

    assert_eq!(position(&runtime, app).1, 1);
}

struct ProgressFile(PathBuf);

impl Drop for ProgressFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

#[test]
fn reopening_restores_a_later_chapter_image_and_page_turns_still_work() {
    use crate::{
        checkpoint_reader, install_reader_images, load_initial_reader,
        reader_progress::ReaderProgress,
    };

    let book = illustrated_book(
        "<p>start</p>",
        r#"<img src="red.png"/><img src="blue.png"/><img src="red.png"/>"#,
        &[
            ("red.png", png(120, 60, [200, 10, 20])),
            ("blue.png", png(120, 60, [10, 20, 200])),
        ],
    );
    let progress = ReaderProgress::open(&book.0).unwrap();
    let _cleanup = ProgressFile(progress.path().to_path_buf());
    drop(progress);

    let saved = {
        let mut host = book.open();
        let mut progress = ReaderProgress::open(&book.0).unwrap();
        let slots = [HostImageSlot::default()];
        let ids = [ImageId::new(0)];
        let (session, images) = load_initial_reader(&mut host, Some(&mut progress), &ids).unwrap();
        install_reader_images(&slots, images);
        let (mut runtime, app) = app_with(session);

        let request = turn_to_boundary(&mut runtime, app, Button::Next);
        load_requested_chapter(&mut runtime, app, Some(&mut host), request, &slots, &ids);
        checkpoint_reader(&runtime, app, Some(&mut progress));

        let request = page_request(&mut runtime, app, Button::Next);
        load_requested_page(&mut runtime, app, Some(&mut host), request, &slots, &ids);
        checkpoint_reader(&runtime, app, Some(&mut progress));
        assert_eq!(position(&runtime, app), (1, 1, 3));

        progress.load().unwrap().unwrap()
    };

    assert_eq!(
        saved.location().offset(),
        inkpaper_epub::ContentOffset::ZERO
    );
    assert_eq!(saved.non_text(), 1);

    let mut host = book.open();
    let mut progress = ReaderProgress::open(&book.0).unwrap();
    let slots = [HostImageSlot::default()];
    let ids = [ImageId::new(0)];
    let (session, images) = load_initial_reader(&mut host, Some(&mut progress), &ids).unwrap();

    assert_eq!(session.spine(), SpineIndex::new(1));
    assert_eq!(session.page_index(), 1);
    assert_eq!(images.len(), 1);
    assert_eq!(images[0].path().as_str(), "blue.png");

    install_reader_images(&slots, images);
    assert_eq!(
        slots[0].pixel(0, 0),
        Some(inkpaper_ui::Color::rgb(10, 20, 200))
    );
    let (mut runtime, app) = app_with(session);
    assert_eq!(displayed_images(&runtime, app)[0].0.as_str(), "blue.png");

    for (button, expected) in [(Button::Previous, 0), (Button::Next, 1)] {
        let request = page_request(&mut runtime, app, button);
        load_requested_page(&mut runtime, app, Some(&mut host), request, &slots, &ids);
        assert_eq!(position(&runtime, app), (1, expected, 3));
    }
}

#[test]
fn reopening_with_a_taller_viewport_keeps_the_original_saved_text_anchor() {
    use crate::{checkpoint_reader, load_initial_reader, reader_progress::ReaderProgress};

    let book = TestBook::new(false);
    let mut progress = ReaderProgress::open(&book.0).unwrap();
    let _cleanup = ProgressFile(progress.path().to_path_buf());

    let saved = {
        let mut host = book.open();
        let (session, _) = load_initial_reader(&mut host, Some(&mut progress), &[]).unwrap();
        let (mut runtime, app) = app_with(session);
        assert_eq!(press(&mut runtime, app, Button::Next), PlatformAction::None);
        checkpoint_reader(&runtime, app, Some(&mut progress));
        progress.load().unwrap().unwrap()
    };
    drop(progress);

    let mut fonts = FontRegistry::<1>::default();
    let font = fonts.register(&BODY_FONT).unwrap();
    let mut host =
        HostReader::open(&book.0, fonts, font, font, Viewport::new(120, 120).unwrap()).unwrap();
    let mut progress = ReaderProgress::open(&book.0).unwrap();
    let (session, _) = load_initial_reader(&mut host, Some(&mut progress), &[]).unwrap();

    assert!(session.current_page().position() < saved);
    assert!(saved < session.current_page().end_position());

    let (runtime, app) = app_with(session);
    checkpoint_reader(&runtime, app, Some(&mut progress));
    progress.flush().unwrap();

    assert_eq!(progress.load().unwrap(), Some(saved));
}

#[test]
fn restore_decodes_only_the_destination_and_failed_restore_preserves_saved_progress() {
    use crate::{checkpoint_reader, load_initial_reader, reader_progress::ReaderProgress};
    use inkpaper_epub::{BookLocation, ContentOffset};

    let mut broken = png(120, 60, [1, 2, 3]);
    broken.truncate(24);
    let book = illustrated_book(
        "<p>start</p>",
        r#"<img src="bad.png"/><img src="good.png"/>"#,
        &[("bad.png", broken), ("good.png", png(120, 60, [4, 5, 6]))],
    );
    let mut progress = ReaderProgress::open(&book.0).unwrap();
    let _cleanup = ProgressFile(progress.path().to_path_buf());
    let location = BookLocation::new(SpineIndex::new(1), ContentOffset::ZERO);
    progress
        .checkpoint(ReadingPosition::new(location, 1))
        .unwrap();

    let mut host = book.open();
    let (session, images) =
        load_initial_reader(&mut host, Some(&mut progress), &[ImageId::new(0)]).unwrap();

    assert_eq!(session.spine(), SpineIndex::new(1));
    assert_eq!(session.page_index(), 1);
    assert_eq!(images.len(), 1);
    assert_eq!(images[0].path().as_str(), "good.png");

    let broken_position = ReadingPosition::new(location, 0);
    progress.checkpoint(broken_position).unwrap();

    let mut host = book.open();
    let (session, images) =
        load_initial_reader(&mut host, Some(&mut progress), &[ImageId::new(0)]).unwrap();

    assert_eq!(session.spine(), SpineIndex::ZERO);
    assert_eq!(session.page_index(), 0);
    assert!(images.is_empty());

    let (runtime, app) = app_with(session);
    checkpoint_reader(&runtime, app, Some(&mut progress));
    progress.flush().unwrap();

    assert_eq!(progress.load().unwrap(), Some(broken_position));
}

#[test]
fn failed_image_turn_does_not_checkpoint_the_requested_page() {
    use crate::{checkpoint_reader, load_initial_reader, reader_progress::ReaderProgress};

    let mut broken = png(120, 60, [1, 2, 3]);
    broken.truncate(24);
    let book = illustrated_book(
        "<p>start</p><img src=\"bad.png\"/>",
        "<p>end</p>",
        &[("bad.png", broken)],
    );
    let mut progress = ReaderProgress::open(&book.0).unwrap();
    let _cleanup = ProgressFile(progress.path().to_path_buf());
    let mut host = book.open();
    let ids = [ImageId::new(0)];
    let slots = [HostImageSlot::default()];
    let (session, _) = load_initial_reader(&mut host, Some(&mut progress), &ids).unwrap();
    let (mut runtime, app) = app_with(session);

    let request = page_request(&mut runtime, app, Button::Next);
    load_requested_page(&mut runtime, app, Some(&mut host), request, &slots, &ids);
    checkpoint_reader(&runtime, app, Some(&mut progress));
    progress.flush().unwrap();

    assert_eq!(position(&runtime, app), (0, 0, 2));
    assert_eq!(progress.load().unwrap(), None);
}

#[test]
fn invalid_saved_spine_falls_back_and_unprepared_images_cannot_enter_a_session() {
    use crate::{load_initial_reader, reader_progress::ReaderProgress};
    use inkpaper_epub::{BookLocation, ContentOffset};

    let book = illustrated_book(
        r#"<img src="good.png"/>"#,
        "<p>end</p>",
        &[("good.png", png(120, 60, [4, 5, 6]))],
    );
    let mut progress = ReaderProgress::open(&book.0).unwrap();
    let _cleanup = ProgressFile(progress.path().to_path_buf());
    let invalid = ReadingPosition::new(
        BookLocation::new(SpineIndex::new(99), ContentOffset::ZERO),
        0,
    );
    progress.checkpoint(invalid).unwrap();

    let mut host = book.open();
    let (session, _) =
        load_initial_reader(&mut host, Some(&mut progress), &[ImageId::new(0)]).unwrap();

    assert_eq!(session.spine(), SpineIndex::ZERO);
    assert_eq!(progress.load().unwrap(), Some(invalid));

    let prepared = host.load_first().unwrap();
    let position = prepared.pagination.pages()[0].position();

    assert!(
        ReaderSession::at_position(
            prepared.pagination,
            prepared.viewport,
            Box::new(()),
            position
        )
        .is_none()
    );
}

#[derive(Default)]
struct HomeTextPainter(Vec<String>);

impl inkpaper_ui::Painter for HomeTextPainter {
    type Error = std::convert::Infallible;

    fn draw_box(
        &mut self,
        _: inkpaper_ui::Rect,
        _: inkpaper_ui::BoxPaint,
        _: Option<inkpaper_ui::Rect>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn draw_canvas(
        &mut self,
        _: inkpaper_ui::Rect,
        _: Option<inkpaper_ui::Rect>,
        _: &mut dyn FnMut(inkpaper_ui::Rect, &mut dyn inkpaper_ui::CanvasPainter),
    ) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl inkpaper_ui::ResourcePainter for HomeTextPainter {
    fn draw_text(
        &mut self,
        _: &mut (),
        text: &str,
        _: inkpaper_ui::Rect,
        _: inkpaper_ui::ResolvedTextStyle,
        _: Option<inkpaper_ui::Rect>,
    ) -> Result<(), Self::Error> {
        self.0.push(text.to_owned());
        Ok(())
    }

    fn draw_image(
        &mut self,
        _: &mut (),
        _: ImageSource,
        _: inkpaper_ui::Rect,
        _: inkpaper_ui::ImagePaint,
        _: Option<inkpaper_ui::Rect>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
}

fn home_text(runtime: &mut TestRuntime, app: Entity<InkPaperApp>) -> Vec<String> {
    InkPaperApp::handle_event(
        runtime,
        app,
        AppEvent::Input(InputEvent::Touch(TouchEvent::HomeTap)),
    );

    runtime.rebuild().unwrap();
    runtime
        .layout_with_measurer(Size::new(px(480), px(800)), &TestMeasurer)
        .unwrap();

    let mut painter = HomeTextPainter::default();
    runtime.paint(&mut painter).unwrap().unwrap();
    painter.0
}

#[test]
fn home_shows_active_metadata_and_tracks_navigation_and_restoration() {
    let book = TestBook::new(false);
    let mut host = book.open();
    let (session, _) = host.load_first().unwrap().into_app_session(&[]).unwrap();
    let (mut runtime, app) = app_with(session);

    runtime
        .update(app, |app, cx| {
            app.model_mut().set_current_book(host.book_summary());
            cx.notify();
        })
        .unwrap();

    assert_eq!(press(&mut runtime, app, Button::Next), PlatformAction::None);
    let before = position(&runtime, app);
    let text = home_text(&mut runtime, app);

    assert!(text.iter().any(|text| text == "Test"));
    assert!(text.iter().any(|text| text == "Unknown author"));
    assert!(text.contains(&format!("Page 2 of {} in this chapter", before.2)));
    assert!(!text.iter().any(|text| text == "0%" || text == "68%"));
    assert_eq!(position(&runtime, app), before);

    // the card is Home's first focusable control.
    assert_eq!(press(&mut runtime, app, Button::Next), PlatformAction::None);
    press(&mut runtime, app, Button::Activate);
    InkPaperApp::handle_event(
        &mut runtime,
        app,
        AppEvent::Input(InputEvent::Button(ButtonEvent::new(
            Button::Activate,
            ButtonEdge::Released,
        ))),
    );

    assert_eq!(
        runtime.update(app, |app, _| app.route()).unwrap(),
        Route::Reader
    );
    assert_eq!(position(&runtime, app), before);

    let request = turn_to_boundary(&mut runtime, app, Button::Next);
    load_requested_chapter(&mut runtime, app, Some(&mut host), request, &[], &[]);
    assert_eq!(position(&runtime, app).0, 3);

    let text = home_text(&mut runtime, app);
    assert!(text.contains(&format!(
        "Page 1 of {} in this chapter",
        position(&runtime, app).2
    )));

    runtime
        .update(app, |app, cx| app.navigate(Route::Reader, cx))
        .unwrap();
    press(&mut runtime, app, Button::Next);

    let saved = runtime
        .update(app, |app, _| {
            app.reader().unwrap().current_page().position()
        })
        .unwrap();
    let expected = position(&runtime, app);
    drop(runtime);
    drop(host);

    let mut reopened = book.open();
    let (session, _) = reopened
        .load_position(saved)
        .unwrap()
        .into_app_session(&[])
        .unwrap();
    let (mut runtime, app) = app_with(session);

    runtime
        .update(app, |app, cx| {
            app.model_mut().set_current_book(reopened.book_summary());
            cx.notify();
        })
        .unwrap();

    let text = home_text(&mut runtime, app);
    assert_eq!(position(&runtime, app), expected);
    assert!(text.contains(&format!(
        "Page {} of {} in this chapter",
        expected.1 + 1,
        expected.2
    )));
    assert!(text.iter().any(|text| text == "Test"));
}

fn metadata_book(metadata: &str) -> TestBook {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "inkpaper-metadata-{}-{}.epub",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));

    let container = br#"<container><rootfiles><rootfile full-path="book.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#;
    let package = format!(
        r#"<package version="3.0" xmlns="http://www.idpf.org/2007/opf"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/">{metadata}</metadata><manifest><item id="a" href="a.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="a"/></spine></package>"#
    );

    fs::write(
        &path,
        stored_zip(&[
            ("mimetype", b"application/epub+zip"),
            ("META-INF/container.xml", container),
            ("book.opf", package.as_bytes()),
            ("a.xhtml", b"<html><body><p>Text</p></body></html>"),
        ]),
    )
    .unwrap();

    TestBook(path)
}

#[test]
fn host_extracts_book_metadata_and_handles_missing_and_oversized_values() {
    let book = metadata_book(
        "<dc:title> A &amp; B </dc:title><dc:creator> </dc:creator><dc:creator> Ada </dc:creator><dc:creator>Grace</dc:creator>",
    );
    let summary = book.open().book_summary();

    assert_eq!(summary.title(), "A & B");
    assert_eq!(summary.author(), "Ada");

    let missing = metadata_book("").open().book_summary();
    assert_eq!(missing.title(), "Untitled book");
    assert_eq!(missing.author(), "Unknown author");

    let long = metadata_book(&format!(
        "<dc:title>{}</dc:title><dc:creator>{}</dc:creator>",
        "é".repeat(100),
        "🙂".repeat(50)
    ))
    .open()
    .book_summary();

    assert!(long.title().len() <= 96);
    assert!(long.author().len() <= 64);
    assert!(long.title().ends_with('…'));
    assert!(long.author().ends_with('…'));
}

fn notice(runtime: &TestRuntime, app: Entity<InkPaperApp>) -> Option<ReaderNotice> {
    runtime
        .update(app, |app, _| app.reader().unwrap().notice())
        .unwrap()
}

fn reader_text(runtime: &mut TestRuntime) -> Vec<String> {
    runtime.rebuild().unwrap();
    runtime
        .layout_with_measurer(Size::new(px(120), px(60)), &TestMeasurer)
        .unwrap();

    let mut painter = HomeTextPainter::default();
    runtime.paint(&mut painter).unwrap().unwrap();
    painter.0
}

#[test]
fn book_boundaries_redraw_a_notice_and_successful_navigation_clears_it() {
    let book = illustrated_book("<p>first</p>", "<p>last</p>", &[]);
    let mut host = book.open();
    let (session, _) = host.load_first().unwrap().into_app_session(&[]).unwrap();
    let page = session.current_page().clone();
    let (mut runtime, app) = app_with(session);
    runtime.take_render_invalidation();

    let request = turn_to_boundary(&mut runtime, app, Button::Previous);
    load_requested_chapter(&mut runtime, app, Some(&mut host), request, &[], &[]);

    assert_eq!(notice(&runtime, app), Some(ReaderNotice::BeginningOfBook));
    assert_eq!(
        runtime.take_render_invalidation().kind(),
        inkpaper_ui::Invalidation::Rebuild
    );
    assert!(
        reader_text(&mut runtime)
            .iter()
            .any(|text| text == "Beginning of book")
    );
    runtime
        .update(app, |app, _| {
            assert_eq!(app.reader().unwrap().current_page(), &page)
        })
        .unwrap();

    let request = turn_to_boundary(&mut runtime, app, Button::Next);
    load_requested_chapter(&mut runtime, app, Some(&mut host), request, &[], &[]);
    assert_eq!(notice(&runtime, app), None);
    assert_eq!(position(&runtime, app).0, 1);

    for _ in 0..2 {
        let request = turn_to_boundary(&mut runtime, app, Button::Next);
        load_requested_chapter(&mut runtime, app, Some(&mut host), request, &[], &[]);
        assert_eq!(notice(&runtime, app), Some(ReaderNotice::EndOfBook));
        assert_eq!(position(&runtime, app), (1, 0, 1));
    }

    assert!(
        reader_text(&mut runtime)
            .iter()
            .any(|text| text == "End of book")
    );

    let request = turn_to_boundary(&mut runtime, app, Button::Previous);
    load_requested_chapter(&mut runtime, app, Some(&mut host), request, &[], &[]);
    assert_eq!(notice(&runtime, app), None);
}

#[test]
fn broken_chapters_are_failures_and_allow_retry_or_reverse() {
    let book = TestBook::new(true);
    let mut host = book.open();
    let (session, _) = host.load_first().unwrap().into_app_session(&[]).unwrap();
    let (mut runtime, app) = app_with(session);

    let request = turn_to_boundary(&mut runtime, app, Button::Next);
    let before = position(&runtime, app);
    load_requested_chapter(&mut runtime, app, Some(&mut host), request, &[], &[]);

    assert_eq!(notice(&runtime, app), Some(ReaderNotice::ChapterLoadFailed));
    assert!(
        reader_text(&mut runtime)
            .iter()
            .any(|text| text == "Could not load chapter. Try again.")
    );

    let request = turn_to_boundary(&mut runtime, app, Button::Next);
    load_requested_chapter(&mut runtime, app, Some(&mut host), request, &[], &[]);

    assert_eq!(position(&runtime, app), before);
    assert_eq!(notice(&runtime, app), Some(ReaderNotice::ChapterLoadFailed));

    press(&mut runtime, app, Button::Previous);
    assert_eq!(position(&runtime, app).1, before.1 - 1);
    assert_eq!(notice(&runtime, app), None);
}

#[test]
fn unavailable_platform_is_a_failure_and_retry_can_install_the_chapter() {
    let book = illustrated_book("<p>first</p>", "<p>last</p>", &[]);
    let mut host = book.open();
    let (session, _) = host.load_first().unwrap().into_app_session(&[]).unwrap();
    let (mut runtime, app) = app_with(session);

    let request = turn_to_boundary(&mut runtime, app, Button::Next);
    load_requested_chapter::<1>(&mut runtime, app, None, request, &[], &[]);
    assert_eq!(notice(&runtime, app), Some(ReaderNotice::ChapterLoadFailed));

    let request = turn_to_boundary(&mut runtime, app, Button::Next);
    load_requested_chapter(&mut runtime, app, Some(&mut host), request, &[], &[]);

    assert_eq!(notice(&runtime, app), None);
    assert_eq!(position(&runtime, app).0, 1);
}

#[test]
fn image_failure_preserves_pixels_and_checkpoint_and_touch_retries_through_notice() {
    use crate::{
        checkpoint_reader, install_reader_images, load_initial_reader,
        reader_progress::ReaderProgress,
    };

    let book = illustrated_book(
        r#"<img src="red.png"/><img src="blue.png"/>"#,
        "<p>end</p>",
        &[
            ("red.png", png(120, 60, [200, 10, 20])),
            ("blue.png", png(120, 60, [10, 20, 200])),
        ],
    );
    let mut host = book.open();
    let slots = [HostImageSlot::default()];
    let ids = [ImageId::new(0)];

    let mut progress = ReaderProgress::open(&book.0).unwrap();
    let _cleanup = ProgressFile(progress.path().to_path_buf());
    let initial = host
        .load_first()
        .unwrap()
        .into_app_session(&ids)
        .unwrap()
        .0
        .current_page()
        .position();

    progress.checkpoint(initial).unwrap();
    let saved = fs::read(progress.path()).unwrap();

    let (session, images) = load_initial_reader(&mut host, Some(&mut progress), &ids).unwrap();
    let page = session.current_page().clone();
    install_reader_images(&slots, images);
    let (mut runtime, app) = app_with(session);

    let request = page_request(&mut runtime, app, Button::Next);
    load_requested_page(&mut runtime, app, Some(&mut host), request, &slots, &[]);

    assert_eq!(notice(&runtime, app), Some(ReaderNotice::PageLoadFailed));
    checkpoint_reader(&runtime, app, Some(&mut progress));
    assert_eq!(fs::read(progress.path()).unwrap(), saved);
    assert_eq!(
        slots[0].pixel(0, 0),
        Some(inkpaper_ui::Color::rgb(200, 10, 20))
    );
    runtime
        .update(app, |app, _| {
            assert_eq!(app.reader().unwrap().current_page(), &page)
        })
        .unwrap();
    assert!(
        reader_text(&mut runtime)
            .iter()
            .any(|text| text == "Could not load page. Try again.")
    );

    let point = TouchPosition::new(90, 30);
    InkPaperApp::handle_event(
        &mut runtime,
        app,
        AppEvent::Input(InputEvent::Touch(TouchEvent::Down(point))),
    );
    let action = InkPaperApp::handle_event(
        &mut runtime,
        app,
        AppEvent::Input(InputEvent::Touch(TouchEvent::Up(point))),
    );
    let PlatformAction::LoadReaderPage(request) = action else {
        panic!("notice must not block the page-turn region")
    };

    load_requested_page(&mut runtime, app, Some(&mut host), request, &slots, &ids);

    assert_eq!(notice(&runtime, app), None);
    assert_eq!(position(&runtime, app).1, 1);
    assert_eq!(
        slots[0].pixel(0, 0),
        Some(inkpaper_ui::Color::rgb(10, 20, 200))
    );

    checkpoint_reader(&runtime, app, Some(&mut progress));
    assert_ne!(fs::read(progress.path()).unwrap(), saved);
}

#[test]
fn home_clears_notices_and_cancelled_completions_cannot_restore_them() {
    let mut broken = png(120, 60, [1, 2, 3]);
    broken.truncate(24);

    let book = illustrated_book(
        "<p>first</p><img src=\"bad.png\"/>",
        "<p>end</p>",
        &[("bad.png", broken)],
    );
    let mut host = book.open();
    let (session, _) = host.load_first().unwrap().into_app_session(&[]).unwrap();
    let (mut runtime, app) = app_with(session);

    let request = page_request(&mut runtime, app, Button::Next);
    load_requested_page(
        &mut runtime,
        app,
        Some(&mut host),
        request,
        &[HostImageSlot::default()],
        &[ImageId::new(0)],
    );

    assert_eq!(notice(&runtime, app), Some(ReaderNotice::PageLoadFailed));

    let request = page_request(&mut runtime, app, Button::Next);
    InkPaperApp::handle_event(
        &mut runtime,
        app,
        AppEvent::Input(InputEvent::Touch(TouchEvent::HomeTap)),
    );

    assert_eq!(notice(&runtime, app), None);

    runtime
        .update(app, |app, cx| {
            assert!(!app.complete_reader_page(request, PageLoadOutcome::Failed, cx));
            app.navigate(Route::Reader, cx);
            assert!(!app.complete_reader_page(request, PageLoadOutcome::Failed, cx));
            assert!(!app.complete_reader_chapter(
                ChapterRequest {
                    from: SpineIndex::ZERO,
                    direction: ChapterDirection::Next,
                },
                ChapterLoadOutcome::Failed,
                cx
            ));
        })
        .unwrap();

    assert_eq!(notice(&runtime, app), None);
    assert_eq!(position(&runtime, app).1, 0);
}
