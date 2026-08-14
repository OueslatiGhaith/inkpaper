use std::{convert::Infallible, hint::black_box};

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use inkpaper_ui::{BoxPaint, Painter, ResolvedTextStyle, TextMeasurer, prelude::*};

const VIEWPORT: Size = Size::new(px(320), px(240));

type BenchRuntime = Runtime<
    4096,  // entity bytes
    4,     // entity slots
    4096,  // callback bytes
    16,    // callback slots
    2048,  // frame nodes
    32768, // frame text bytes
    1024,  // element states
>;

#[derive(Default)]
struct NullPainter;

impl TextMeasurer for NullPainter {
    fn measure_text(&self, text: &str, _: ResolvedTextStyle, max_size: Size) -> Size {
        let characters = i32::try_from(text.len()).unwrap_or(i32::MAX);

        let width = px(6)
            .saturating_mul(characters)
            .min(max_size.width.non_negative());
        let height = match text.is_empty() {
            true => px(0),
            false => px(10).min(max_size.height.non_negative()),
        };

        Size::new(width, height)
    }
}

impl Painter for NullPainter {
    type Error = Infallible;

    fn draw_box(&mut self, _: Rect, _: BoxPaint, _: Option<Rect>) -> Result<(), Self::Error> {
        Ok(())
    }

    fn draw_text(
        &mut self,
        _: &str,
        _: Rect,
        _: ResolvedTextStyle,
        _: Option<Rect>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn draw_image(
        &mut self,
        _: ImageSource,
        _: Rect,
        _: ImageFit,
        _: Option<Rect>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn draw_canvas(
        &mut self,
        bounds: Rect,
        _: Option<Rect>,
        draw: &mut dyn FnMut(Rect, &mut dyn CanvasPainter),
    ) -> Result<(), Self::Error> {
        let mut canvas = NullCanvasPainter;

        draw(
            Rect::new(Point::ZERO, Size::new(bounds.width(), bounds.height())),
            &mut canvas,
        );

        Ok(())
    }
}

struct NullCanvasPainter;

impl CanvasPainter for NullCanvasPainter {
    fn fill_rect(&mut self, _: Rect, _: Color) {}
    fn stroke_rect(&mut self, _: Rect, _: Pixels, _: Color) {}
    fn line(&mut self, _: Point, _: Point, _: Pixels, _: Color) {}
    fn fill_circle(&mut self, _: Point, _: Pixels, _: Color) {}
    fn stroke_circle(&mut self, _: Point, _: Pixels, _: Pixels, _: Color) {}
}

struct ListApp {
    rows: usize,
}

impl Render for ListApp {
    fn render<'a>(&'a mut self, _: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        div()
            .w_full()
            .flex()
            .flex_col()
            .children((0..self.rows).map(|_| div().w_full().h(px(16)).child("Benchmark row")))
    }
}

fn setup(rows: usize) -> (BenchRuntime, Entity<ListApp>, NullPainter) {
    let mut runtime = BenchRuntime::default();

    let app = runtime.create(move |_| ListApp { rows }).unwrap();
    runtime.rebuild(app).unwrap();

    let painter = NullPainter;
    runtime.layout(VIEWPORT, &painter).unwrap();

    (runtime, app, painter)
}

fn benchmark_rebuild(c: &mut Criterion) {
    let mut group = c.benchmark_group("rebuild");

    for rows in [8usize, 32, 128, 512] {
        group.throughput(Throughput::Elements(rows as u64));

        let (mut runtime, app, _) = setup(rows);

        group.bench_with_input(BenchmarkId::from_parameter(rows), &rows, |bencher, _| {
            bencher.iter(|| {
                runtime.rebuild(black_box(app)).unwrap();

                black_box(runtime.frame_node_count());
            });
        });
    }

    group.finish();
}

fn benchmark_layout(c: &mut Criterion) {
    let mut group = c.benchmark_group("layout");

    for rows in [8usize, 32, 128, 512] {
        group.throughput(Throughput::Elements(rows as u64));

        let (mut runtime, _, painter) = setup(rows);

        group.bench_with_input(BenchmarkId::from_parameter(rows), &rows, |bencher, _| {
            bencher.iter(|| {
                black_box(
                    runtime
                        .layout(black_box(VIEWPORT), black_box(&painter))
                        .unwrap(),
                );
            });
        });
    }

    group.finish();
}

fn benchmark_paint(c: &mut Criterion) {
    let mut group = c.benchmark_group("paint");

    for rows in [8usize, 32, 128, 512] {
        group.throughput(Throughput::Elements(rows as u64));

        let (runtime, _, mut painter) = setup(rows);

        group.bench_with_input(BenchmarkId::from_parameter(rows), &rows, |bencher, _| {
            bencher.iter(|| {
                black_box(runtime.paint(black_box(&mut painter)).unwrap());
            });
        });
    }

    group.finish();
}

fn benchmark_full_frame(c: &mut Criterion) {
    let mut group = c.benchmark_group("full_frame");

    for rows in [8usize, 32, 128, 512] {
        group.throughput(Throughput::Elements(rows as u64));

        let (mut runtime, app, mut painter) = setup(rows);

        group.bench_with_input(BenchmarkId::from_parameter(rows), &rows, |bencher, _| {
            bencher.iter(|| {
                runtime.rebuild(black_box(app)).unwrap();

                runtime
                    .layout(black_box(VIEWPORT), black_box(&painter))
                    .unwrap();

                runtime.paint(black_box(&mut painter)).unwrap();
            });
        });
    }

    group.finish();
}

criterion_group!(
    rendering,
    benchmark_rebuild,
    benchmark_layout,
    benchmark_paint,
    benchmark_full_frame,
);

criterion_main!(rendering);
