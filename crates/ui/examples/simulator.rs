use std::fmt::Write;

use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::OriginDimensions,
    image::{GetPixel, ImageDrawable},
    mono_font::{
        MonoFont,
        ascii::{FONT_6X10, FONT_10X20},
    },
    pixelcolor::{Rgb888, RgbColor},
    prelude::{Pixel as EgPixel, Point as EgPoint, Size as EgSize},
    primitives::Rectangle,
};
use embedded_graphics_simulator::{
    OutputSettingsBuilder, SimulatorDisplay, SimulatorEvent, Window,
    sdl2::{Keycode, MouseButton},
};
use heapless::String;
use inkpaper_ui::{
    DamageRegion, Offset,
    backend::{EmbeddedGraphicsImage, EmbeddedGraphicsPainter},
    prelude::*,
};

const DISPLAY_WIDTH: u32 = 320;
const DISPLAY_HEIGHT: u32 = 240;
const DISPLAY_SIZE_EG: EgSize = EgSize::new(DISPLAY_WIDTH, DISPLAY_HEIGHT);
const DISPLAY_SIZE: Size = Size::new(px(DISPLAY_WIDTH as i32), px(DISPLAY_HEIGHT as i32));

type UiRuntime = Runtime<
    16_384, // entity bytes
    32,     // entity slots
    8_192,  // callback bytes
    64,     // callback slots
    256,    // frame nodes
    4_096,  // frame text bytes
    128,    // persistent element states
>;

const FONTS: [&MonoFont; 2] = [&FONT_6X10, &FONT_10X20];

const DEMO_IMAGE_WIDTH: u32 = 48;
const DEMO_IMAGE_HEIGHT: u32 = 24;

const DEMO_IMAGE_SOURCE: ImageSource = ImageSource::new(
    ImageId::new(0),
    Size::new(px(DEMO_IMAGE_WIDTH as i32), px(DEMO_IMAGE_HEIGHT as i32)),
);

struct DemoImage;

static DEMO_IMAGE: DemoImage = DemoImage;

impl OriginDimensions for DemoImage {
    fn size(&self) -> EgSize {
        EgSize::new(DEMO_IMAGE_WIDTH, DEMO_IMAGE_HEIGHT)
    }
}

impl GetPixel for DemoImage {
    type Color = Rgb888;

    fn pixel(&self, point: EgPoint) -> Option<Self::Color> {
        if point.x < 0 || point.y < 0 {
            return None;
        }

        let x = u32::try_from(point.x).ok()?;
        let y = u32::try_from(point.y).ok()?;
        if x >= DEMO_IMAGE_WIDTH || y >= DEMO_IMAGE_HEIGHT {
            return None;
        }

        let color = if x == y.saturating_mul(2)
            || x.saturating_add(y.saturating_mul(2)) == DEMO_IMAGE_WIDTH - 1
        {
            Rgb888::new(245, 245, 245)
        } else if x < 16 {
            Rgb888::new(55, 122, 190)
        } else if x < 32 {
            Rgb888::new(58, 151, 105)
        } else {
            Rgb888::new(202, 126, 65)
        };

        Some(color)
    }
}

impl ImageDrawable for DemoImage {
    type Color = Rgb888;

    fn draw<D>(&self, target: &mut D) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        let pixels = (0..DEMO_IMAGE_HEIGHT as i32).flat_map(|y| {
            (0..DEMO_IMAGE_WIDTH as i32).filter_map(move |x| {
                let point = EgPoint::new(x, y);

                self.pixel(point).map(|color| EgPixel(point, color))
            })
        });

        target.draw_iter(pixels)
    }

    fn draw_sub_image<D>(&self, target: &mut D, _: &Rectangle) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        self.draw(target)
    }
}

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
            .gap(px(4))
            .bg(Color::rgb(30, 36, 48))
            .border(px(1))
            .border_color(Color::rgb(65, 74, 92))
            .rounded(px(6))
            .child(
                text(self.title)
                    .font(FontId::new(1))
                    .text_color(Color::WHITE),
            )
            .child(
                text(self.subtitle)
                    .wrap()
                    .line_height(px(12))
                    .text_color(Color::rgb(169, 179, 197)),
            )
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

    fn increment(&mut self, _: &ActivateEvent, cx: &mut Context<Self>) {
        self.value = self.value.saturating_add(1);
        self.update_label();
        cx.notify();
    }

    fn decrement(&mut self, _: &ActivateEvent, cx: &mut Context<Self>) {
        self.value = self.value.saturating_sub(1);
        self.update_label();
        cx.notify();
    }

    fn reset(&mut self, _: &ActivateEvent, cx: &mut Context<Self>) {
        self.value = 0;
        self.update_label();
        cx.notify();
    }

    fn draw_meter(&self, bounds: Rect, painter: &mut dyn CanvasPainter) {
        painter.fill_rect(bounds, Color::rgb(25, 30, 39));
        painter.stroke_rect(bounds, px(1), Color::rgb(86, 100, 126));

        let active = self.value.min(10);
        for index in 0..10 {
            let x = px(8 + index * 25);
            let color = if index < active as i32 {
                Color::rgb(54, 170, 105)
            } else {
                Color::rgb(49, 57, 72)
            };

            painter.fill_rect(
                Rect::new(Point::new(x, px(8)), Size::new(px(18), px(16))),
                color,
            );
            painter.stroke_rect(
                Rect::new(Point::new(x, px(8)), Size::new(px(18), px(16))),
                px(1),
                Color::rgb(91, 105, 130),
            );
        }

        let marker_index = active.min(9);
        let marker_x = px(17 + i32::try_from(marker_index).unwrap_or(9) * 25);

        painter.line(
            Point::new(px(8), px(31)),
            Point::new(px(251), px(31)),
            px(1),
            Color::rgb(69, 80, 101),
        );
        painter.fill_circle(Point::new(marker_x, px(31)), px(3), Color::WHITE);
        painter.stroke_circle(
            Point::new(marker_x, px(31)),
            px(5),
            px(1),
            Color::rgb(90, 140, 220),
        );
    }
}

impl Render for Counter {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        let increment = cx.listener(Self::increment);
        let decrement = cx.listener(Self::decrement);
        let reset = cx.listener(Self::reset);
        let meter = cx.canvas(Self::draw_meter);

        div()
            .w_full()
            .p(px(12))
            .gap(px(10))
            .bg(Color::rgb(48, 57, 72))
            .border(px(1))
            .border_color(Color::rgb(69, 80, 101))
            .rounded(px(6))
            .child("Interaction + Focus + Canvas")
            .child(self.label.as_str())
            .child(meter.size(Size::new(px(260), px(40))))
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
                            .on_activate(decrement)
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
                            .on_activate(reset)
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
                            .on_activate(increment)
                            .child("+1"),
                    ),
            )
    }
}

fn section_title(label: &'static str) -> impl IntoElement {
    text(label).text_color(Color::rgb(225, 231, 240))
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

fn scroll_row(
    id: &'static str,
    label: &'static str,
    color: Color,
    listener: Listener<ActivateEvent>,
) -> impl IntoElement {
    div()
        .id(id)
        .w_full()
        .h(px(24))
        .p(px(6))
        .bg(color)
        .border(px(1))
        .border_color(color)
        .rounded(px(3))
        .when_focused(|style| style.border_color(Color::WHITE))
        .on_activate(listener)
        .child(label)
}

fn image_fit_card(label: &'static str, fit: ImageFit) -> impl IntoElement {
    div()
        .w(px(88))
        .p(px(4))
        .gap(px(4))
        .bg(Color::rgb(28, 33, 43))
        .border(px(1))
        .border_color(Color::rgb(73, 84, 106))
        .rounded(px(4))
        .child(
            text(label)
                .text_center()
                .text_color(Color::rgb(193, 203, 219)),
        )
        .child(
            image(DEMO_IMAGE_SOURCE)
                .size(Size::new(px(78), px(52)))
                .fit(fit),
        )
}

struct App {
    header: Entity<Header>,
    counter: Entity<Counter>,
    show_details: bool,
}

impl App {
    fn new(cx: &mut Context<Self>) -> Self {
        let header = cx.new(|_| Header::new("InkPaper UI", "Feature gallery - mouse wheel scrolls, Tab/arrows move focus, Enter/Space activates, C clears focus.")).unwrap();
        let counter = cx.new(|_| Counter::new()).unwrap();

        Self {
            header,
            counter,
            show_details: false,
        }
    }

    fn demo_clicked(&mut self, _: &ActivateEvent, _: &mut Context<Self>) {}

    fn toggle_details(&mut self, _: &ActivateEvent, cx: &mut Context<Self>) {
        self.show_details = !self.show_details;
        cx.notify();
    }
}

fn text_styling_section() -> impl IntoElement {
    div()
        .w_full()
        .p(px(8))
        .gap(px(6))
        .bg(Color::rgb(37, 43, 55))
        .border(px(1))
        .border_color(Color::rgb(62, 72, 91))
        .rounded(px(6))
        .child(section_title("Text styling + inheritance"))
        .child(
            div()
                .w_full()
                .p(px(6))
                .gap(px(4))
                .bg(Color::rgb(27, 31, 41))
                .rounded(px(4))
                .text_color(Color::rgb(108, 190, 144))
                .line_height(px(13))
                .child("Inherited green text and custom line height")
                .child(
                    text("Local override on the child")
                    .text_color(Color::rgb(230, 167, 87)),
                ),
        )
        .child(
            div()
                .w_full()
                .p(px(6))
                .bg(Color::rgb(27, 31, 41))
                .rounded(px(4))
                .child(
                    text(
                        "Secondary 10x20 font",
                    )
                    .font(FontId::new(1))
                    .text_color(
                        Color::rgb(
                            126,
                            172,
                            235,
                        ),
                    ),
                ),
        )
        .child(
            div()
                .w_full()
                .p(px(6))
                .bg(Color::rgb(27, 31, 41))
                .rounded(px(4))
                .child(
                    text(
                        "Word wrapping is shared by measurement and painting. This deliberately long sentence is clamped to two lines and ends with an ellipsis when more content remains.",
                    )
                    .wrap()
                    .max_lines(2)
                    .text_ellipsis()
                    .text_color(Color::rgb(190, 201, 221)),
                ),
        )
        .child(
            div()
                .w_full()
                .p(px(6))
                .bg(Color::rgb(27, 31, 41))
                .rounded(px(4))
                .child(
                    text("Centered line\nsecond line")
                    .text_center()
                    .text_color(Color::rgb(126, 172, 235)),
                ),
        )
}

fn images_section() -> impl IntoElement {
    div()
        .w_full()
        .p(px(8))
        .gap(px(6))
        .bg(Color::rgb(37, 43, 55))
        .border(px(1))
        .border_color(Color::rgb(62, 72, 91))
        .rounded(px(6))
        .child(section_title("Images + fitting"))
        .child(
            text("Same 48x24 procedural image rendered into three 78x52 boxes.")
                .wrap()
                .text_color(Color::rgb(164, 176, 196)),
        )
        .child(
            div()
                .w_full()
                .flex()
                .gap(px(5))
                .child(image_fit_card("contain", ImageFit::Contain))
                .child(image_fit_card("cover", ImageFit::Cover))
                .child(image_fit_card("fill", ImageFit::Fill)),
        )
        .child(
            div()
                .w_full()
                .p(px(5))
                .gap(px(4))
                .bg(Color::rgb(27, 31, 41))
                .rounded(px(4))
                .child(text("Native size").text_color(Color::rgb(190, 201, 221)))
                .child(image(DEMO_IMAGE_SOURCE).native()),
        )
}

fn alignment_section() -> impl IntoElement {
    div()
        .w_full()
        .p(px(7))
        .gap(px(5))
        .bg(Color::rgb(37, 43, 55))
        .border(px(1))
        .border_color(Color::rgb(62, 72, 91))
        .rounded(px(6))
        .child(section_title("Alignment + margins + min/max"))
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
        )
}

fn weighted_flex_section() -> impl IntoElement {
    div()
        .w_full()
        .p(px(7))
        .gap(px(5))
        .bg(Color::rgb(37, 43, 55))
        .border(px(1))
        .border_color(Color::rgb(62, 72, 91))
        .rounded(px(6))
        .child(section_title("Weighted flex grow: 1 : 2 : 1"))
        .child(
            div()
                .w_full()
                .flex()
                .gap(px(4))
                .child(weighted_flex_block("1x", 1, Color::rgb(68, 91, 148)))
                .child(weighted_flex_block("2x", 2, Color::rgb(75, 126, 101)))
                .child(weighted_flex_block("1x", 1, Color::rgb(139, 91, 67))),
        )
}

fn overflow_clipping_section() -> impl IntoElement {
    div()
        .w_full()
        .p(px(7))
        .gap(px(5))
        .bg(Color::rgb(37, 43, 55))
        .border(px(1))
        .border_color(Color::rgb(62, 72, 91))
        .rounded(px(6))
        .child(section_title("overflow_hidden clipping"))
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
        )
}

fn nested_scroll_section(demo_click: Listener<ActivateEvent>) -> impl IntoElement {
    div()
        .w_full()
        .p(px(7))
        .gap(px(5))
        .bg(Color::rgb(37, 43, 55))
        .border(px(1))
        .border_color(Color::rgb(62, 72, 91))
        .rounded(px(6))
        .child(
            section_title("Nested scroll + focus into view")
        )
        .child(
            text(
                "Tab or arrow through these rows. Focusing an off-screen row should scroll it into view."
            )
            .wrap()
            .text_color(Color::rgb(164, 176, 196)),
        )
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
                .child(
                    scroll_row(
                        "scroll-row-1",
                        "Row 1 - focus me",
                        Color::rgb(58, 76, 105),
                        demo_click,
                    ),
                )
                .child(
                    scroll_row(
                        "scroll-row-2",
                        "Row 2 - focus scroll",
                        Color::rgb(61, 91, 83),
                        demo_click,
                    ),
                )
                .child(
                    scroll_row(
                        "scroll-row-3",
                        "Row 3 - focus scroll",
                        Color::rgb(94, 76, 58),
                        demo_click,
                    ),
                )
                .child(
                    scroll_row(
                        "scroll-row-4",
                        "Row 4 - nested focus",
                        Color::rgb(76, 65, 102),
                        demo_click,
                    ),
                )
                .child(
                    scroll_row(
                        "scroll-row-5",
                        "Row 5 - nested focus",
                        Color::rgb(104, 62, 77),
                        demo_click,
                    ),
                )
                .child(
                    scroll_row(
                        "scroll-row-6",
                        "Row 6 - end",
                        Color::rgb(54, 94, 111),
                        demo_click,
                    ),
                ),
        )
}

fn gallery_footer() -> impl IntoElement {
    div()
        .w_full()
        .p(px(8))
        .gap(px(4))
        .mb(px(8))
        .bg(Color::rgb(30, 36, 48))
        .border(px(1))
        .border_color(Color::rgb(65, 74, 92))
        .rounded(px(6))
        .child(
            text("End of InkPaper UI feature gallery")
                .text_center()
                .text_color(Color::rgb(190, 200, 216)),
        )
        .child(
            text("Rendering uses damage-aware partial painting.")
                .text_center()
                .text_color(Color::rgb(132, 145, 166)),
        )
}

fn conditional_section(show_details: bool, toggle: Listener<ActivateEvent>) -> impl IntoElement {
    let button_label = if show_details {
        "Hide conditional child"
    } else {
        "Show conditional child"
    };

    div()
        .w_full()
        .p(px(7))
        .gap(px(6))
        .bg(Color::rgb(37, 43, 55))
        .border(px(1))
        .border_color(Color::rgb(62, 72, 91))
        .rounded(px(6))
        .child(section_title("Conditional rendering"))
        .child(
            text(
                "This exercises Entity state, Context::notify(), rebuilding and ConditionalElementExt::when().",
            )
            .wrap()
            .text_color(Color::rgb(164, 176, 196)),
        )
        .child(
            div()
                .id("toggle-details")
                .w_full()
                .h(px(28))
                .flex()
                .items_center()
                .justify_center()
                .bg(Color::rgb(58, 91, 146))
                .border(px(1))
                .border_color(Color::rgb(92,130,190))
                .rounded(px(4))
                .when_focused(|style| style.border_color(Color::WHITE))
                .when_pressed(
                    |style| style.bg(Color::rgb(47, 119, 91)))
                .on_activate(toggle)
                .child(button_label),
        )
        .when(
            show_details,
            |section| {
                section.child(
                    div()
                        .w_full()
                        .p(px(8))
                        .gap(px(4))
                        .bg(Color::rgb(28,52,43))
                        .border(px(1))
                        .border_color(Color::rgb(72,145,111))
                        .rounded(px(4))
                        .child(
                            text("Conditional child mounted")
                            .text_color(Color::rgb(156,225,184)),
                        )
                        .child(
                            text(
                                "Toggle the button again and this subtree disappears from the next frame.",
                            )
                            .wrap()
                            .text_color(Color::rgb(174,195,183)),
                        ),
                )
            },
        )
}

fn positioning_section(demo_click: Listener<ActivateEvent>) -> impl IntoElement {
    div()
        .w_full()
        .p(px(7))
        .gap(px(6))
        .bg(Color::rgb(37, 43, 55))
        .border(px(1))
        .border_color(Color::rgb(62, 72, 91))
        .rounded(px(6))
        .child(section_title("Positioning + overlays"))
        .child(
            text(
                "Relative positioning keeps its flow slot. Absolute positioning is removed from flow and uses the nearest positioned ancestor.",
            )
            .wrap()
            .text_color(Color::rgb(164, 176, 196)),
        )
        .child(
            div()
                .relative()
                .w_full()
                .h(px(112))
                .p(px(6))
                .gap(px(4))
                .bg(Color::rgb(25, 29, 38))
                .border(px(1))
                .border_color(Color::rgb(86, 100, 126))
                .rounded(px(4))
                // these 3 children participate in normal block flow.
                // the middle one is visually shifted, but the third child still 
                // occupies the slot immediately after its original unshifted flow position.
                .child(
                    div()
                        .w(px(116))
                        .h(px(22))
                        .p(px(5))
                        .bg(Color::rgb(58, 76, 105))
                        .rounded(px(3))
                        .child("normal flow"),
                )
                .child(
                    div()
                        .relative()
                        .left(px(18))
                        .top(px(3))
                        .w(px(116))
                        .h(px(22))
                        .p(px(5))
                        .bg(Color::rgb(61, 104, 83))
                        .border(px(1))
                        .border_color(Color::rgb(96, 164, 128))
                        .rounded(px(3))
                        .child("relative +18,+3"),
                )
                .child(
                    div()
                        .w(px(116))
                        .h(px(22))
                        .p(px(5))
                        .bg(Color::rgb(94, 76, 58))
                        .rounded(px(3))
                        .child("next flow slot"),
                )
                // this is completely outside normal flow.
                // the stage is `.relative()`, so it establishes the containing
                // block used by this absolute child.
                // taking it interactive also exercises positioned hit testing
                // and keyboard focus.
                .child(
                    div()
                        .id("absolute-overlay")
                        .absolute()
                        .top(px(6))
                        .right(px(6))
                        .w(px(92))
                        .h(px(28))
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(Color::rgb(123, 70, 117))
                        .border(px(1))
                        .border_color(Color::rgb(166, 98, 157))
                        .rounded(px(4))
                        .when_focused(|style| {
                            style.border(px(2)).border_color(Color::WHITE)
                        })
                        .when_pressed(|style| style.bg(Color::rgb(151, 76, 101)))
                        .on_activate(demo_click)
                        .child("absolute"),
                )
                // width remains Auto. left + right therefore stretch the element 
                // across the containing block.
                .child(
                    div()
                        .absolute()
                        .left(px(6))
                        .right(px(6))
                        .bottom(px(6))
                        .h(px(18))
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(Color::rgb(56, 73, 94))
                        .border(px(1))
                        .border_color(Color::rgb(89, 113, 145))
                        .rounded(px(3))
                        .child("left + right = stretched auto width"),
                ),
        )
        .child(
            text(
                "The shifted green row overlaps without moving the next row; the purple button and bottom bar occupy no flow space.",
            )
            .wrap()
            .text_color(Color::rgb(142, 155, 176)),
        )
}

impl Render for App {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        let demo_click = cx.listener(Self::demo_clicked);

        let toggle_details = cx.listener(Self::toggle_details);

        div()
            .id("page")
            .w_full()
            .h_full()
            .flex_col()
            .p(px(8))
            .gap(px(8))
            .bg(Color::rgb(18, 21, 28))
            .overflow_y_scroll()
            .child(self.header)
            .child(self.counter)
            .child(conditional_section(self.show_details, toggle_details))
            .child(text_styling_section())
            .child(images_section())
            .child(alignment_section())
            .child(weighted_flex_section())
            .child(positioning_section(demo_click))
            .child(overflow_clipping_section())
            .child(nested_scroll_section(demo_click))
            .child(gallery_footer())
    }
}

fn to_ui_point(point: EgPoint) -> Point {
    Point::new(px(point.x), px(point.y))
}

fn update_ui(runtime: &mut UiRuntime, app: Entity<App>, display: &mut SimulatorDisplay<Rgb888>) {
    let invalidation = runtime.take_render_invalidation();
    match invalidation.kind() {
        Invalidation::None => {}
        Invalidation::Paint => paint_ui(runtime, display, invalidation.damage()),
        Invalidation::Layout => {
            layout_ui(runtime, display);
            paint_ui(runtime, display, invalidation.damage());
        }
        Invalidation::Rebuild => rebuild_ui(runtime, app, display),
    }
}

fn layout_ui(runtime: &mut UiRuntime, display: &mut SimulatorDisplay<Rgb888>) {
    let painter = EmbeddedGraphicsPainter::new(display, FONTS, []);
    runtime.layout(DISPLAY_SIZE, &painter).unwrap();
}

fn paint_ui(runtime: &mut UiRuntime, display: &mut SimulatorDisplay<Rgb888>, damage: DamageRegion) {
    if damage.is_none() {
        return;
    }

    let demo_image: EmbeddedGraphicsImage<'static, SimulatorDisplay<Rgb888>> =
        EmbeddedGraphicsImage::new(&DEMO_IMAGE);
    let mut painter = EmbeddedGraphicsPainter::new(display, FONTS, [demo_image]);

    painter.clear_damage(damage, Color::BLACK).unwrap();
    runtime
        .paint_with_damage(damage, &mut painter)
        .unwrap()
        .unwrap();
}

fn rebuild_ui(runtime: &mut UiRuntime, app: Entity<App>, display: &mut SimulatorDisplay<Rgb888>) {
    runtime.rebuild(app).unwrap();
    layout_ui(runtime, display);
    paint_ui(runtime, display, DamageRegion::full());
}

fn handle_key_down(runtime: &mut UiRuntime, keycode: Keycode) {
    match keycode {
        Keycode::Down | Keycode::Right | Keycode::Tab => {
            runtime.focus_next();
        }
        Keycode::Up | Keycode::Left => {
            runtime.focus_previous();
        }
        Keycode::Return | Keycode::Space => {
            runtime.begin_focused_activation();
        }
        Keycode::C => {
            runtime.cancel_activation();
            runtime.clear_focus();
        }
        _ => {}
    };
}

fn handle_key_up(runtime: &mut UiRuntime, keycode: Keycode) {
    match keycode {
        Keycode::Return | Keycode::Space => {
            runtime.complete_focused_activation().unwrap();
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
    let mut window = Window::new(
        "InkPaper UI | wheel • tab/arrows • enter/space",
        &output_settings,
    );

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
                    runtime.begin_activation_at(mouse_position);
                }
                SimulatorEvent::MouseButtonUp {
                    mouse_btn: MouseButton::Left,
                    point,
                } => {
                    mouse_position = to_ui_point(point);
                    runtime.complete_activation_at(mouse_position).unwrap();
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
                } => handle_key_down(&mut runtime, keycode),
                SimulatorEvent::KeyUp { keycode, .. } => handle_key_up(&mut runtime, keycode),
                _ => {}
            }
        }

        update_ui(&mut runtime, app, &mut display);
    }
}
