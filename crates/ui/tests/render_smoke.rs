use embedded_graphics::{mono_font::ascii::FONT_6X10, pixelcolor::Rgb888, prelude::Size as EgSize};
use embedded_graphics_simulator::SimulatorDisplay;
use inkpaper_ui::{
    backend::{EmbeddedGraphicsPainter, MonoFontFace},
    prelude::*,
};

const DISPLAY_WIDTH: u32 = 128;
const DISPLAY_HEIGHT: u32 = 64;

type TestRuntime = Runtime<4096, 16, 4096, 16, 64, 1024, 32>;

struct App;

impl Render for App {
    fn render<'a>(&'a mut self, _cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        div()
            .w_full()
            .h_full()
            .p(px(4))
            .bg(Color::rgb(20, 20, 20))
            .text_color(Color::WHITE)
            .child(
                div()
                    .p(px(4))
                    .bg(Color::rgb(50, 70, 90))
                    .child("Hello from the UI"),
            )
    }
}

static TEST_FONT_FACE: MonoFontFace<'static> = MonoFontFace::new(&FONT_6X10);

fn test_font_resources() -> FontResources<'static, 1, 64, 4096> {
    let mut resources = FontResources::default();

    let id = resources.register(&TEST_FONT_FACE).unwrap();
    assert_eq!(id, FontId::DEFAULT,);

    resources
}

#[test]
fn full_runtime_can_render_to_simulator_display() {
    let mut runtime = TestRuntime::default();

    let app = runtime.create(|_| App).unwrap();
    runtime.rebuild(app).unwrap();

    let mut display = SimulatorDisplay::<Rgb888>::new(EgSize::new(DISPLAY_WIDTH, DISPLAY_HEIGHT));

    let mut fonts = test_font_resources();
    let mut painter =
        EmbeddedGraphicsPainter::new(&mut display, &mut fonts, ImageRegistry::<0>::default());

    let laid_out = runtime.layout(
        Size::new(px(DISPLAY_WIDTH as i32), px(DISPLAY_HEIGHT as i32)),
        &painter,
    );

    assert_eq!(
        laid_out,
        Some(Size::new(
            px(DISPLAY_WIDTH as i32),
            px(DISPLAY_HEIGHT as i32),
        ))
    );

    let painted = runtime.paint(&mut painter).unwrap();

    assert_eq!(painted, Some(()));
    assert!(runtime.frame_node_count() > 0);
    assert!(runtime.frame_text_bytes_used() > 0);
}
