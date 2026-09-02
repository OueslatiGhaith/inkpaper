use embedded_graphics::{mono_font::ascii::FONT_6X10, pixelcolor::Rgb888, prelude::Size as EgSize};
use embedded_graphics_simulator::SimulatorDisplay;
use inkpaper_ui::{
    backend::{EmbeddedGraphicsPainter, MonoFontFace},
    prelude::*,
};

const DISPLAY_WIDTH: u32 = 128;
const DISPLAY_HEIGHT: u32 = 64;

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

#[test]
fn full_runtime_can_render_to_simulator_display() {
    let mut runtime = RuntimeBuilder::default()
        .entities::<4096, 16>()
        .callbacks::<4096, 16>()
        .frame::<64, 1024>()
        .element_states::<32>()
        .render_resources::<1, 64, 4096, 0>()
        .build();

    assert_eq!(
        runtime.register_font(&TEST_FONT_FACE).unwrap(),
        FontId::DEFAULT,
    );

    runtime.create_root(|_| App).unwrap();
    runtime.rebuild().unwrap();

    let mut display = SimulatorDisplay::<Rgb888>::new(EgSize::new(DISPLAY_WIDTH, DISPLAY_HEIGHT));

    let laid_out = runtime.layout(Size::new(
        px(DISPLAY_WIDTH as i32),
        px(DISPLAY_HEIGHT as i32),
    ));

    assert_eq!(
        laid_out,
        Some(Size::new(
            px(DISPLAY_WIDTH as i32),
            px(DISPLAY_HEIGHT as i32),
        )),
    );

    let mut painter = EmbeddedGraphicsPainter::new(&mut display);

    let painted = runtime.paint(&mut painter).unwrap();

    assert_eq!(painted, Some(()));
    assert!(runtime.frame_node_count() > 0);
    assert!(runtime.frame_text_bytes_used() > 0);
}
