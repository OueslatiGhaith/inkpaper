use std::fmt::Write;

use embedded_graphics::{
    draw_target::DrawTarget,
    mono_font::ascii::FONT_6X10,
    pixelcolor::{Rgb888, RgbColor},
    prelude::{Point as EgPoint, Size as EgSize},
};
use embedded_graphics_simulator::{
    OutputSettingsBuilder, SimulatorDisplay, SimulatorEvent, Window,
    sdl2::{Keycode, MouseButton},
};
use heapless::String;
use inkpaper_ui::{Offset, backend::EmbeddedGraphicsPainter, prelude::*};

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
            .border(px(1))
            .border_color(Color::rgb(65, 74, 92))
            .rounded(px(6))
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
        write!(&mut self.label, "Count: {}", self.value,).unwrap();
    }

    fn increment(&mut self, _: &ClickEvent, cx: &mut Context<Self>) {
        self.value = self.value.saturating_add(1);
        self.update_label();
        cx.notify();
    }

    fn decrement(&mut self, _: &ClickEvent, cx: &mut Context<Self>) {
        self.value = self.value.saturating_sub(1);
        self.update_label();
        cx.notify();
    }

    fn reset(&mut self, _: &ClickEvent, cx: &mut Context<Self>) {
        self.value = 0;
        self.update_label();
        cx.notify();
    }
}

impl Render for Counter {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        let increment = cx.listener(Self::increment);
        let decrement = cx.listener(Self::decrement);
        let reset = cx.listener(Self::reset);

        div()
            .w_full()
            .p(px(12))
            .gap(px(10))
            .bg(Color::rgb(48, 57, 72))
            .border(px(1))
            .border_color(Color::rgb(69, 80, 101))
            .rounded(px(6))
            .child("Interaction + Focus")
            .child(self.label.as_str())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6))
                    .w_full()
                    .child(
                        div()
                            .id("decrement")
                            .w(px(54))
                            .h(px(28))
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(Color::rgb(77, 84, 102))
                            .border(px(1))
                            .border_color(Color::rgb(105, 115, 138))
                            .rounded(px(5))
                            .when_focused(|style| style.border_color(Color::WHITE))
                            .when_pressed(|style| style.bg(Color::rgb(108, 62, 70)))
                            .on_click(decrement)
                            .child("-1"),
                    )
                    .child(
                        div()
                            .id("reset")
                            .w(px(62))
                            .h(px(28))
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(Color::rgb(100, 76, 45))
                            .border(px(1))
                            .border_color(Color::rgb(149, 112, 61))
                            .rounded(px(5))
                            .when_focused(|style| style.w(px(76)).border_color(Color::WHITE))
                            .when_pressed(|style| style.bg(Color::rgb(128, 90, 45)))
                            .on_click(reset)
                            .child("Reset"),
                    )
                    .child(
                        div()
                            .id("increment")
                            .flex_1()
                            .min_w(px(70))
                            .h(px(28))
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(Color::rgb(55, 105, 180))
                            .border(px(1))
                            .border_color(Color::rgb(90, 140, 220))
                            .rounded(px(5))
                            .when_focused(|style| style.border(px(2)).border_color(Color::WHITE))
                            .when_pressed(|style| style.bg(Color::rgb(48, 138, 92)).min_w(px(82)))
                            .on_click(increment)
                            .child("+1"),
                    ),
            )
    }
}

fn weighted_flex_block(label: &'static str, grow: u16, color: Color) -> impl IntoElement {
    div()
        .flex_basis(px(0))
        .flex_grow(grow)
        .h(px(26))
        .flex()
        .items_center()
        .justify_center()
        .bg(color)
        .border(px(1))
        .border_color(Color::rgb(100, 112, 136))
        .rounded(px(4))
        .child(label)
}

fn scroll_row(label: &'static str, color: Color) -> impl IntoElement {
    div()
        .w_full()
        .h(px(24))
        .p(px(6))
        .bg(color)
        .rounded(px(3))
        .child(label)
}

struct App {
    header: Entity<Header>,
    counter: Entity<Counter>,
}

impl App {
    fn new(cx: &mut Context<Self>) -> Self {
        let header = cx.new(|_| Header::new("InkPaper UI", "Gallery")).unwrap();
        let counter = cx.new(|_| Counter::new()).unwrap();

        Self { header, counter }
    }
}

impl Render for App {
    fn render<'a>(&'a mut self, _: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        div()
            .id("page")
            .w_full()
            .h_full()
            .flex_col()
            .p(px(8))
            .gap(px(8))
            .bg(Color::rgb(18, 21, 28))
            .overflow_y_scroll()
            // persistent child entities
            .child(self.header)
            .child(self.counter)
            // alignment, justification, margins and min/max
            .child(
                div()
                    .w_full()
                    .p(px(7))
                    .gap(px(5))
                    .bg(Color::rgb(37, 43, 55))
                    .border(px(1))
                    .border_color(Color::rgb(62, 72, 91))
                    .rounded(px(6))
                    .child("Alignment + margins + min/max")
                    .child(
                        div()
                            .w_full()
                            .h(px(38))
                            .flex()
                            .items_center()
                            .justify_between()
                            .bg(Color::rgb(28, 32, 42))
                            .rounded(px(4))
                            .child(
                                div()
                                    .min_w(px(54))
                                    .max_w(px(72))
                                    .h(px(20))
                                    .ml(px(5))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .bg(Color::rgb(68, 89, 130))
                                    .rounded(px(3))
                                    .child("min/max"),
                            )
                            .child(
                                div()
                                    .w(px(26))
                                    .h(px(26))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .bg(Color::rgb(68, 130, 104))
                                    .rounded(px(13))
                                    .child("C"),
                            )
                            .child(
                                div()
                                    .w(px(48))
                                    .h(px(16))
                                    .mr(px(5))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .bg(Color::rgb(130, 84, 68))
                                    .rounded(px(3))
                                    .child("end"),
                            ),
                    ),
            )
            // explicit weighted flex
            .child(
                div()
                    .w_full()
                    .p(px(7))
                    .gap(px(5))
                    .bg(Color::rgb(37, 43, 55))
                    .border(px(1))
                    .border_color(Color::rgb(62, 72, 91))
                    .rounded(px(6))
                    .child("Weighted flex grow: 1 : 2 : 1")
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .gap(px(4))
                            .child(weighted_flex_block("1x", 1, Color::rgb(68, 91, 148)))
                            .child(weighted_flex_block("2x", 2, Color::rgb(75, 126, 101)))
                            .child(weighted_flex_block("1x", 1, Color::rgb(139, 91, 67))),
                    ),
            )
            // pure clipping demonstration.
            // the second row extends outside the 34px viewport.
            .child(
                div()
                    .w_full()
                    .p(px(7))
                    .gap(px(5))
                    .bg(Color::rgb(37, 43, 55))
                    .border(px(1))
                    .border_color(Color::rgb(62, 72, 91))
                    .rounded(px(6))
                    .child("overflow_hidden clipping")
                    .child(
                        div()
                            .w_full()
                            .h(px(34))
                            .overflow_hidden()
                            .border(px(1))
                            .border_color(Color::rgb(96, 108, 131))
                            .rounded(px(4))
                            .child(
                                div()
                                    .w_full()
                                    .h(px(22))
                                    .p(px(5))
                                    .bg(Color::rgb(65, 104, 145))
                                    .child("Visible child"),
                            )
                            .child(
                                div()
                                    .w_full()
                                    .h(px(22))
                                    .p(px(5))
                                    .bg(Color::rgb(145, 73, 75))
                                    .child("Partly clipped child"),
                            ),
                    ),
            )
            // nested scrolling. Because hit testing chooses the
            // deepest scroll target, the wheel scrolls this list
            // when the mouse is over it, and the page otherwise.
            .child(
                div()
                    .w_full()
                    .p(px(7))
                    .gap(px(5))
                    .bg(Color::rgb(37, 43, 55))
                    .border(px(1))
                    .border_color(Color::rgb(62, 72, 91))
                    .rounded(px(6))
                    .child("Nested persistent scroll area")
                    .child(
                        div()
                            .id("nested-scroll")
                            .w_full()
                            .h(px(76))
                            .gap(px(3))
                            .p(px(3))
                            .bg(Color::rgb(25, 29, 38))
                            .border(px(1))
                            .border_color(Color::rgb(86, 100, 126))
                            .rounded(px(4))
                            .overflow_y_scroll()
                            .child(scroll_row("Row 1 - persistent", Color::rgb(58, 76, 105)))
                            .child(scroll_row("Row 2 - clipped", Color::rgb(61, 91, 83)))
                            .child(scroll_row("Row 3 - scroll", Color::rgb(94, 76, 58)))
                            .child(scroll_row(
                                "Row 4 - survives rebuild",
                                Color::rgb(76, 65, 102),
                            ))
                            .child(scroll_row("Row 5 - paint only", Color::rgb(104, 62, 77)))
                            .child(scroll_row("Row 6 - end", Color::rgb(54, 94, 111))),
                    ),
            )
            // bottom marker makes page scrolling obvious.
            .child(
                div()
                    .w_full()
                    .h(px(28))
                    .flex()
                    .items_center()
                    .justify_center()
                    .mb(px(8))
                    .bg(Color::rgb(30, 36, 48))
                    .border(px(1))
                    .border_color(Color::rgb(65, 74, 92))
                    .rounded(px(6))
                    .child("End of feature gallery"),
            )
    }
}

fn to_ui_point(point: EgPoint) -> Point {
    Point::new(px(point.x), px(point.y))
}

fn update_ui(runtime: &mut UiRuntime, app: Entity<App>, display: &mut SimulatorDisplay<Rgb888>) {
    match runtime.take_invalidation() {
        Invalidation::None => {}
        Invalidation::Paint => paint_ui(runtime, display),
        Invalidation::Layout => {
            layout_ui(runtime, display);
            paint_ui(runtime, display);
        }
        Invalidation::Rebuild => rebuild_ui(runtime, app, display),
    }
}

fn layout_ui(runtime: &mut UiRuntime, display: &mut SimulatorDisplay<Rgb888>) {
    let painter = EmbeddedGraphicsPainter::new(display, [&FONT_6X10]);
    runtime.layout(DISPLAY_SIZE, &painter).unwrap();
}

fn paint_ui(runtime: &mut UiRuntime, display: &mut SimulatorDisplay<Rgb888>) {
    display.clear(Rgb888::BLACK).unwrap();

    let mut painter = EmbeddedGraphicsPainter::new(display, [&FONT_6X10]);
    runtime.paint(&mut painter).unwrap().unwrap();
}

fn rebuild_ui(runtime: &mut UiRuntime, app: Entity<App>, display: &mut SimulatorDisplay<Rgb888>) {
    runtime.rebuild(app).unwrap();
    layout_ui(runtime, display);
    paint_ui(runtime, display);
}

fn handle_key(runtime: &mut UiRuntime, keycode: Keycode) {
    match keycode {
        Keycode::Down | Keycode::Right | Keycode::Tab => {
            runtime.focus_next();
        }
        Keycode::Up | Keycode::Left => {
            runtime.focus_previous();
        }
        Keycode::Return | Keycode::Space => {
            runtime.activate_focused().unwrap();
        }
        Keycode::Escape => {
            runtime.clear_focus();
        }
        _ => {}
    }
}

fn main() {
    let mut runtime = UiRuntime::default();

    let app = runtime.create(App::new).unwrap();
    let mut display = SimulatorDisplay::<Rgb888>::new(DISPLAY_SIZE_EG);

    rebuild_ui(&mut runtime, app, &mut display);

    let output_settings = OutputSettingsBuilder::new().scale(3).build();
    let mut window = Window::new("InkPaper UI", &output_settings);

    let mut mouse_position = Point::ZERO;

    'running: loop {
        window.update(&display);

        for event in window.events() {
            match event {
                SimulatorEvent::Quit => break 'running,
                SimulatorEvent::MouseMove { point } => mouse_position = to_ui_point(point),
                SimulatorEvent::MouseButtonDown {
                    mouse_btn: MouseButton::Left,
                    point,
                } => {
                    mouse_position = to_ui_point(point);
                    runtime.pointer_down(mouse_position);
                }
                SimulatorEvent::MouseButtonUp {
                    mouse_btn: MouseButton::Left,
                    point,
                } => {
                    mouse_position = to_ui_point(point);
                    runtime.pointer_up(mouse_position).unwrap();
                }
                SimulatorEvent::MouseWheel { scroll_delta, .. } => {
                    const SCROLL_STEP: i32 = 12;
                    runtime.scroll_at(
                        mouse_position,
                        Offset::new(
                            px(-scroll_delta.x.saturating_mul(SCROLL_STEP)),
                            px(-scroll_delta.y.saturating_mul(SCROLL_STEP)),
                        ),
                    );
                }
                SimulatorEvent::KeyDown {
                    keycode,
                    repeat: false,
                    ..
                } => handle_key(&mut runtime, keycode),
                _ => {}
            }
        }

        update_ui(&mut runtime, app, &mut display);
    }
}
