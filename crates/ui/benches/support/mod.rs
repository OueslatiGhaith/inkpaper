#![allow(dead_code)]

use std::convert::Infallible;

use inkpaper_ui::{
    BoxPaint, Element, MountCx, MountError, NodeId, Painter, ResolvedTextStyle, TextMeasurer,
    prelude::*,
};

pub const VIEWPORT: Size = Size::new(px(320), px(240));

pub const FLAT_LIST_SIZES: &[usize] = &[8, 32, 128, 512];
pub const FLEX_ROW_SIZES: &[usize] = &[8, 32, 128, 512];
pub const NESTED_SIZES: &[usize] = &[4, 16, 64, 256];
pub const TEXT_HEAVY_SIZES: &[usize] = &[8, 32, 128, 512];
pub const SCROLL_LIST_SIZES: &[usize] = &[8, 32, 128, 512];
pub const MIXED_SCREEN_SIZES: &[usize] = &[8, 32, 128];

pub const REPRESENTATIVE_CASES: &[(BenchScenario, &[usize])] = &[
    (BenchScenario::FlexRow, FLEX_ROW_SIZES),
    (BenchScenario::Nested, NESTED_SIZES),
    (BenchScenario::TextHeavy, TEXT_HEAVY_SIZES),
    (BenchScenario::ScrollList, SCROLL_LIST_SIZES),
    (BenchScenario::MixedScreen, MIXED_SCREEN_SIZES),
];

pub const ALL_CASES: &[(BenchScenario, &[usize])] = &[
    (BenchScenario::FlatList, FLAT_LIST_SIZES),
    (BenchScenario::FlexRow, FLEX_ROW_SIZES),
    (BenchScenario::Nested, NESTED_SIZES),
    (BenchScenario::TextHeavy, TEXT_HEAVY_SIZES),
    (BenchScenario::ScrollList, SCROLL_LIST_SIZES),
    (BenchScenario::MixedScreen, MIXED_SCREEN_SIZES),
];

pub type BenchRuntime = Runtime<
    4096,  // entity bytes
    4,     // entity slots
    4096,  // callback bytes
    16,    // callback slots
    2048,  // frame nodes
    32768, // frame text bytes
    1024,  // element states
>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BenchScenario {
    FlatList,
    FlexRow,
    Nested,
    TextHeavy,
    ScrollList,
    MixedScreen,
}

impl BenchScenario {
    pub const fn name(self) -> &'static str {
        match self {
            Self::FlatList => "flat_list",
            Self::FlexRow => "flex_row",
            Self::Nested => "nested",
            Self::TextHeavy => "text_heavy",
            Self::ScrollList => "scroll_list",
            Self::MixedScreen => "mixed_screen",
        }
    }
}

#[derive(Default)]
pub struct BenchPainter {
    draw_calls: usize,
}

impl BenchPainter {
    pub fn draw_calls(&self) -> usize {
        self.draw_calls
    }

    fn record_draw(&mut self) {
        self.draw_calls = self.draw_calls.wrapping_add(1);
    }
}

impl TextMeasurer for BenchPainter {
    fn measure_text(&self, text: &str, _: ResolvedTextStyle, max_size: Size) -> Size {
        let characters = i32::try_from(text.len()).unwrap_or(i32::MAX);

        let width = px(6)
            .saturating_mul(characters)
            .min(max_size.width.non_negative());

        let height = if text.is_empty() {
            px(0)
        } else {
            px(10).min(max_size.height.non_negative())
        };

        Size::new(width, height)
    }
}

impl Painter for BenchPainter {
    type Error = Infallible;

    fn draw_box(&mut self, _: Rect, _: BoxPaint, _: Option<Rect>) -> Result<(), Self::Error> {
        self.record_draw();
        Ok(())
    }

    fn draw_text(
        &mut self,
        _: &str,
        _: Rect,
        _: ResolvedTextStyle,
        _: Option<Rect>,
    ) -> Result<(), Self::Error> {
        self.record_draw();
        Ok(())
    }

    fn draw_image(
        &mut self,
        _: ImageSource,
        _: Rect,
        _: ImageFit,
        _: Option<Rect>,
    ) -> Result<(), Self::Error> {
        self.record_draw();
        Ok(())
    }

    fn draw_canvas(
        &mut self,
        bounds: Rect,
        _: Option<Rect>,
        draw: &mut dyn FnMut(Rect, &mut dyn CanvasPainter),
    ) -> Result<(), Self::Error> {
        self.record_draw();

        let mut painter = BenchCanvasPainter;

        draw(
            Rect::new(Point::ZERO, Size::new(bounds.width(), bounds.height())),
            &mut painter,
        );

        Ok(())
    }
}

struct BenchCanvasPainter;

impl CanvasPainter for BenchCanvasPainter {
    fn fill_rect(&mut self, _: Rect, _: Color) {}
    fn stroke_rect(&mut self, _: Rect, _: Pixels, _: Color) {}
    fn line(&mut self, _: Point, _: Point, _: Pixels, _: Color) {}
    fn fill_circle(&mut self, _: Point, _: Pixels, _: Color) {}
    fn stroke_circle(&mut self, _: Point, _: Pixels, _: Pixels, _: Color) {}
}

pub struct BenchApp {
    scenario: BenchScenario,
    size: usize,
}

impl Render for BenchApp {
    fn render<'a>(&'a mut self, _: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        BenchScene {
            scenario: self.scenario,
            size: self.size,
        }
    }
}

struct BenchScene {
    scenario: BenchScenario,
    size: usize,
}

impl Element for BenchScene {
    fn mount(self, cx: &mut MountCx<'_>) -> Result<NodeId, MountError> {
        match self.scenario {
            BenchScenario::FlatList => flat_list(self.size).mount(cx),
            BenchScenario::FlexRow => flex_row(self.size).mount(cx),
            BenchScenario::Nested => nested(self.size).mount(cx),
            BenchScenario::TextHeavy => text_heavy(self.size).mount(cx),
            BenchScenario::ScrollList => scroll_list(self.size).mount(cx),
            BenchScenario::MixedScreen => mixed_screen(self.size).mount(cx),
        }
    }
}

fn flat_list(rows: usize) -> impl Element {
    div()
        .w_full()
        .flex()
        .flex_col()
        .children((0..rows).map(|_| div().w_full().h(px(16)).child("Benchmark row")))
}

fn flex_row(items: usize) -> impl Element {
    div()
        .w_full()
        .h(px(64))
        .flex()
        .flex_row()
        .children((0..items).map(|_| {
            div()
                .w(px(24))
                .h_full()
                .flex_basis(px(24))
                .flex_grow(1)
                .flex_shrink(1)
        }))
}

fn nested(items: usize) -> impl Element {
    div().w_full().children((0..items).map(|_| nested_item()))
}

fn nested_item() -> impl Element {
    div().w_full().p(px(1)).child(
        div().w_full().p(px(1)).child(
            div().w_full().p(px(1)).child(
                div()
                    .w_full()
                    .p(px(1))
                    .child(div().w_full().p(px(1)).child("Nested leaf")),
            ),
        ),
    )
}

fn text_heavy(rows: usize) -> impl Element {
    const LONG_TEXT: &str = "Long benchmark text wraps across a constrained embedded row.";

    div().w_full().children((0..rows).map(|_| {
        div()
            .w(px(120))
            .p(px(2))
            .wrap()
            .max_lines(3)
            .text_ellipsis()
            .child(LONG_TEXT)
    }))
}

fn scroll_list(rows: usize) -> impl Element {
    div()
        .w_full()
        .h(px(120))
        .flex()
        .flex_col()
        .id("benchmark-scroll")
        .overflow_y_scroll()
        .children((0..rows).map(|_| div().w_full().h(px(16)).child("Scrollable row")))
}

fn mixed_screen(rows: usize) -> impl Element {
    div()
        .w_full()
        .h_full()
        .p(px(4))
        .gap(px(4))
        .flex()
        .flex_col()
        .child(mixed_header())
        .child(mixed_stats())
        .child(mixed_settings(rows))
}

fn mixed_header() -> impl Element {
    div()
        .w_full()
        .h(px(24))
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .child("Inkpaper")
        .child("Ready")
}

fn mixed_stats() -> impl Element {
    div()
        .w_full()
        .h(px(48))
        .gap(px(4))
        .flex()
        .flex_row()
        .children((0..3).map(|_| {
            div()
                .h_full()
                .flex_1()
                .p(px(2))
                .border(px(1))
                .border_color(Color::BLACK)
                .rounded(px(2))
                .child("Metric")
        }))
}

fn mixed_settings(rows: usize) -> impl Element {
    div()
        .w_full()
        .flex_1()
        .flex()
        .flex_col()
        .id("mixed-scroll")
        .overflow_y_scroll()
        .children((0..rows).map(|_| mixed_settings_row()))
}

fn mixed_settings_row() -> impl Element {
    div()
        .w_full()
        .h(px(24))
        .px(px(4))
        .border(px(1))
        .border_color(Color::BLACK)
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .child("Setting")
        .child("Value")
}

pub fn setup(
    scenario: BenchScenario,
    size: usize,
) -> (Box<BenchRuntime>, Entity<BenchApp>, BenchPainter) {
    let mut runtime = Box::new(BenchRuntime::default());

    let app = runtime
        .create(move |_| BenchApp { scenario, size })
        .unwrap();

    runtime.rebuild(app).unwrap();

    let painter = BenchPainter::default();

    runtime.layout(VIEWPORT, &painter).unwrap();

    (runtime, app, painter)
}
