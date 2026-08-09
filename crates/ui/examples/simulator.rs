use std::fmt::Write;

use embedded_graphics::{
    draw_target::DrawTarget,
    mono_font::ascii::FONT_6X10,
    pixelcolor::{Rgb888, RgbColor},
    prelude::{Point as EgPoint, Size as EgSize},
};
use embedded_graphics_simulator::{
    OutputSettingsBuilder, SimulatorDisplay, SimulatorEvent, Window, sdl2::MouseButton,
};
use heapless::String;
use inkpaper_ui::{MonoTextPainter, prelude::*};

const DISPLAY_WIDTH: u32 = 320;
const DISPLAY_HEIGHT: u32 = 240;
const DISPLAY_SIZE_EG: EgSize = EgSize::new(DISPLAY_WIDTH, DISPLAY_HEIGHT);
const DISPLAY_SIZE: Size = Size::new(px(DISPLAY_WIDTH as i32), px(DISPLAY_HEIGHT as i32));

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
    fn render<'a>(&'a mut self, _: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        div()
            .w_full()
            .p(px(10))
            .gap(px(3))
            .bg(Color::rgb(30, 36, 48))
            .child(self.title)
            .child(self.subtitle)
    }
}

struct Counter {
    value: u32,
    label: String<32>,
}

impl Counter {
    fn new() -> Self {
        let mut counter = Self {
            value: 0,
            label: String::new(),
        };

        counter.update_label();

        counter
    }

    fn update_label(&mut self) {
        self.label.clear();

        write!(&mut self.label, "Count: {}", self.value,).expect("counter label capacity exceeded");
    }

    fn increment(&mut self, _: &ClickEvent, cx: &mut Context<Self>) {
        self.value = self.value.saturating_add(1);

        self.update_label();

        cx.notify();
    }
}

impl Render for Counter {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        div()
            .w_full()
            .p(px(12))
            .gap(px(10))
            .bg(Color::rgb(48, 57, 72))
            .child(self.label.as_str())
            .child(
                div()
                    .id("increment")
                    .w(px(120))
                    .h(px(34))
                    .p(px(8))
                    .bg(Color::rgb(55, 105, 180))
                    .when_pressed(|style| style.bg(Color::rgb(105, 180, 55)))
                    .on_click(cx.listener(Self::increment))
                    .child("Increment"),
            )
    }
}

struct App {
    header: Entity<Header>,
    counter: Entity<Counter>,
}

impl App {
    fn new(cx: &mut Context<Self>) -> Self {
        let header = cx
            .new(|_| Header::new("InkPaper UI", "Click the buttom below"))
            .unwrap();
        let counter = cx.new(|_| Counter::new()).unwrap();

        Self { header, counter }
    }
}

impl Render for App {
    fn render<'a>(&'a mut self, _: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        div()
            .w_full()
            .h_full()
            .p(px(8))
            .gap(px(8))
            .bg(Color::rgb(18, 21, 28))
            .child(self.header)
            .child(self.counter)
    }
}

fn render_ui(
    runtime: &mut UiRuntime,
    app: Entity<App>,
    display: &mut SimulatorDisplay<Rgb888>,
    painter: &MonoTextPainter,
) {
    runtime.rebuild(app).unwrap();
    runtime.layout(DISPLAY_SIZE, painter).unwrap();

    display.clear(Rgb888::new(0, 0, 0)).unwrap();

    runtime.paint(display, painter).unwrap().unwrap();
}

fn to_ui_point(point: EgPoint) -> Point {
    Point::new(px(point.x), px(point.y))
}

fn update_ui(
    runtime: &mut UiRuntime,
    app: Entity<App>,
    display: &mut SimulatorDisplay<Rgb888>,
    painter: &MonoTextPainter,
) {
    match runtime.take_invalidation() {
        inkpaper_ui::Invalidation::None => {}
        inkpaper_ui::Invalidation::Paint => {
            display.clear(Rgb888::BLACK).unwrap();
            runtime.paint(display, painter).unwrap();
        }
        inkpaper_ui::Invalidation::Layout => {
            runtime.layout(DISPLAY_SIZE, painter).unwrap();
            display.clear(Rgb888::BLACK).unwrap();
            runtime.paint(display, painter).unwrap();
        }
        inkpaper_ui::Invalidation::Rebuild => {
            runtime.rebuild(app).unwrap();
            runtime.layout(DISPLAY_SIZE, painter).unwrap();
            display.clear(Rgb888::BLACK).unwrap();
            runtime.paint(display, painter).unwrap();
        }
    }
}

fn main() {
    let mut runtime = UiRuntime::default();

    let app = runtime.create(App::new).unwrap();
    let painter = MonoTextPainter::new(&FONT_6X10, Color::WHITE);
    let mut display = SimulatorDisplay::<Rgb888>::new(DISPLAY_SIZE_EG);

    render_ui(&mut runtime, app, &mut display, &painter);

    let output_settings = OutputSettingsBuilder::new().scale(3).build();
    let mut window = Window::new("InkPaper UI", &output_settings);

    'running: loop {
        window.update(&display);

        for event in window.events() {
            match event {
                SimulatorEvent::Quit => break 'running,
                SimulatorEvent::MouseButtonDown {
                    mouse_btn: MouseButton::Left,
                    point,
                } => {
                    runtime.pointer_down(to_ui_point(point));
                }
                SimulatorEvent::MouseButtonUp {
                    mouse_btn: MouseButton::Left,
                    point,
                } => {
                    runtime.pointer_up(to_ui_point(point)).unwrap();
                }
                _ => {}
            }
        }

        update_ui(&mut runtime, app, &mut display, &painter);
    }
}
