use embedded_graphics::{mono_font::ascii::FONT_6X10, pixelcolor::Rgb888, prelude::Size as EgSize};
use embedded_graphics_simulator::{OutputSettingsBuilder, SimulatorDisplay, Window};
use inkpaper_ui::{MonoTextPainter, prelude::*};

type UiRuntime = Runtime<
    16_384, // entity bytes
    32,     // entity slots
    8_192,  // listener bytes
    64,     // listener slots
    256,    // frame nodes
    4_096,  // frame text bytes
    128,    // persistent element states
>;

struct Header {
    title: &'static str,
    subtitle: &'static str,
}

impl Header {
    fn new(title: &'static str, subtitle: &'static str) -> Self {
        Self { title, subtitle }
    }
}

impl Render for Header {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        div()
            .w_full()
            .p(px(10))
            .gap(px(3))
            .bg(Color::rgb(30, 36, 48))
            .child(self.title)
            .child(self.subtitle)
    }
}

struct StatCard {
    label: &'static str,
    value: &'static str,
    background: Color,
}

impl StatCard {
    fn new(label: &'static str, value: &'static str, background: Color) -> Self {
        Self {
            label,
            value,
            background,
        }
    }
}

impl Render for StatCard {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        div()
            .w_full()
            .p(px(8))
            .gap(px(4))
            .bg(self.background)
            .child(self.label)
            .child(self.value)
    }
}

struct App {
    header: Entity<Header>,
    temperature: Entity<StatCard>,
    humidity: Entity<StatCard>,
    pressure: Entity<StatCard>,
}

impl App {
    fn new(cx: &mut Context<Self>) -> Self {
        let header = cx
            .new(|_| Header::new("InkPaper UI", "embedded-graphics"))
            .unwrap();
        let temperature = cx
            .new(|_| StatCard::new("Temperature", "23 C", Color::rgb(48, 70, 96)))
            .unwrap();
        let humidity = cx
            .new(|_| StatCard::new("Humidity", "54%", Color::rgb(50, 82, 72)))
            .unwrap();
        let pressure = cx
            .new(|_| StatCard::new("Pressure", "1013 hPa", Color::rgb(76, 65, 89)))
            .unwrap();

        Self {
            header,
            temperature,
            humidity,
            pressure,
        }
    }
}

impl Render for App {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        div()
            .w_full()
            .h_full()
            .p(px(8))
            .gap(px(8))
            .bg(Color::rgb(18, 21, 28))
            .child(self.header)
            .child(
                div()
                    .flex()
                    .w_full()
                    .gap(px(8))
                    .child(self.temperature)
                    .child(self.humidity),
            )
            .child(self.pressure)
    }
}

fn main() {
    const DISPLAY_WIDTH: u32 = 320;
    const DISPLAY_HEIGHT: u32 = 240;

    let mut runtime = UiRuntime::default();
    let text_painter = MonoTextPainter::new(&FONT_6X10, Color::WHITE);

    let app = runtime.create(App::new).unwrap();
    runtime.rebuild(app).unwrap();

    runtime
        .layout(
            Size::new(px(DISPLAY_WIDTH as i32), px(DISPLAY_HEIGHT as i32)),
            &text_painter,
        )
        .unwrap();

    let mut display = SimulatorDisplay::<Rgb888>::new(EgSize::new(DISPLAY_WIDTH, DISPLAY_HEIGHT));

    runtime.paint(&mut display, &text_painter).unwrap().unwrap();

    let output_settings = OutputSettingsBuilder::new().scale(3).build();
    let mut widnow = Window::new("InkPaper UI", &output_settings);

    widnow.show_static(&display);
}
