use embedded_graphics::{
    geometry::Size as EgSize, mono_font::ascii::FONT_6X10, pixelcolor::Rgb888,
};
use embedded_graphics_simulator::SimulatorDisplay;
use heapless::String;
use inkpaper_ui::{
    backend::{EmbeddedGraphicsPainter, MonoFontFace},
    prelude::*,
};

const DISPLAY_WIDTH: u32 = 128;
const DISPLAY_HEIGHT: u32 = 64;

type TestRuntime = Runtime<
    4096, // entity bytes
    1,    // entity slots
    4096, // callback bytes
    16,   // callback slots
    64,   // frame nodes
    1024, // frame text bytes
    32,   // element states
>;

struct Badge<'a> {
    label: &'a str,
    color: Color,
}

impl RenderOnce for Badge<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        div()
            .p(px(4))
            .bg(self.color)
            .rounded(px(3))
            .child(self.label)
    }
}

struct Card<'a> {
    title: &'a str,
    badge: &'a str,
}

impl RenderOnce for Card<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        div()
            .p(px(4))
            .gap(px(4))
            .bg(Color::rgb(30, 30, 30))
            .child(self.title)
            .child(Badge {
                label: self.badge,
                color: Color::rgb(60, 120, 80),
            })
    }
}

struct App {
    title: String<32>,
    status: String<32>,
}

impl App {
    fn new() -> Self {
        let mut title = String::new();
        title.push_str("Device").unwrap();

        let mut status = String::new();
        status.push_str("Connected").unwrap();

        Self { title, status }
    }
}

impl Render for App {
    fn render<'a>(&'a mut self, _: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        div().w_full().h_full().p(px(4)).child(Card {
            title: self.title.as_str(),
            badge: self.status.as_str(),
        })
    }
}

#[test]
fn render_once_component_can_be_used_as_a_child() {
    let mut runtime = TestRuntime::default();

    let app = runtime.create(|_| App::new()).unwrap();

    runtime.rebuild(app).unwrap();

    assert!(runtime.frame_node_count() > 1);
    assert!(runtime.frame_text_bytes_used() > 0);
}

static TEST_FONT_FACE: MonoFontFace<'static> = MonoFontFace::new(&FONT_6X10);

fn test_font_resources() -> FontResources<'static, 1, 64, 4096> {
    let mut resources = FontResources::default();

    let id = resources.register(&TEST_FONT_FACE).unwrap();
    assert_eq!(id, FontId::DEFAULT,);

    resources
}

#[test]
fn render_once_component_can_borrow_from_persistent_component() {
    let mut runtime = TestRuntime::default();

    let app = runtime.create(|_| App::new()).unwrap();

    runtime.rebuild(app).unwrap();

    let mut display = SimulatorDisplay::<Rgb888>::new(EgSize::new(DISPLAY_WIDTH, DISPLAY_HEIGHT));

    let mut fonts = test_font_resources();
    let mut painter =
        EmbeddedGraphicsPainter::new(&mut display, &mut fonts, ImageRegistry::<0>::default());

    let size = runtime.layout(
        Size::new(px(DISPLAY_WIDTH as i32), px(DISPLAY_HEIGHT as i32)),
        &painter,
    );

    assert_eq!(
        size,
        Some(Size::new(
            px(DISPLAY_WIDTH as i32),
            px(DISPLAY_HEIGHT as i32),
        ))
    );
    assert_eq!(runtime.paint(&mut painter).unwrap(), Some(()));
}

#[test]
fn render_once_components_can_nest_other_render_once_components() {
    let mut runtime = TestRuntime::default();

    let app = runtime.create(|_| App::new()).unwrap();

    runtime.rebuild(app).unwrap();

    let mut display = SimulatorDisplay::<Rgb888>::new(EgSize::new(DISPLAY_WIDTH, DISPLAY_HEIGHT));

    let mut fonts = test_font_resources();
    let mut painter =
        EmbeddedGraphicsPainter::new(&mut display, &mut fonts, ImageRegistry::<0>::default());

    runtime
        .layout(
            Size::new(px(DISPLAY_WIDTH as i32), px(DISPLAY_HEIGHT as i32)),
            &painter,
        )
        .unwrap();

    runtime.paint(&mut painter).unwrap().unwrap();

    assert!(runtime.frame_node_count() >= 4);
}
