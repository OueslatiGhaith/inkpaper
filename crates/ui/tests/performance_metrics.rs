#![cfg(feature = "metrics")]

use std::convert::Infallible;

use inkpaper_ui::{BoxPaint, Painter, ResolvedTextStyle, TextMeasurer, prelude::*};

type TestRuntime = Runtime<4096, 4, 4096, 16, 64, 512, 32>;

struct TestPainter;

impl TextMeasurer for TestPainter {
    fn measure_text(&self, text: &str, _: ResolvedTextStyle, max_size: Size) -> Size {
        let width = px(i32::try_from(text.len()).unwrap_or(i32::MAX))
            .saturating_mul(6)
            .min(max_size.width.non_negative());

        let height = if text.is_empty() {
            px(0)
        } else {
            px(10).min(max_size.height.non_negative())
        };

        Size::new(width, height)
    }
}

impl Painter for TestPainter {
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
        _: Rect,
        _: Option<Rect>,
        _: &mut dyn FnMut(Rect, &mut dyn CanvasPainter),
    ) -> Result<(), Self::Error> {
        Ok(())
    }
}

struct App;

impl Render for App {
    fn render<'a>(&'a mut self, _: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        div().w(px(100)).h(px(40)).child("Hello")
    }
}

#[test]
fn performance_metrics_track_rebuild_layout_and_paint_work() {
    let mut runtime = TestRuntime::default();

    let app = runtime.create(|_| App).unwrap();

    let painter = TestPainter;

    runtime.reset_performance_metrics();
    runtime.rebuild(app).unwrap();

    let rebuild = runtime.performance_metrics();

    assert_eq!(rebuild.entity_render_calls, 1,);
    assert_eq!(rebuild.nodes_mounted, 3,);

    runtime.reset_performance_metrics();
    runtime
        .layout(Size::new(px(100), px(40)), &painter)
        .unwrap();

    let layout = runtime.performance_metrics();

    assert!(layout.measure_node_calls > 0);
    assert!(layout.measurement_cache_hits > 0);
    assert!(layout.measurement_cache_misses > 0);
    assert_eq!(
        layout.measure_node_calls,
        layout
            .measurement_cache_hits
            .saturating_add(layout.measurement_cache_misses,),
    );
    assert!(layout.text_measurements > 0);
    assert!(layout.flex_base_main_size_calls > 0);
    assert!(layout.nodes_laid_out > 0);

    runtime.reset_performance_metrics();
    runtime.paint(&mut TestPainter).unwrap();

    let paint = runtime.performance_metrics();

    assert_eq!(paint.visual_nodes_visited, 3,);
    assert_eq!(paint.visible_nodes, 3,);
    assert_eq!(paint.nodes_painted, 2,);
}

struct FlexListApp;

impl Render for FlexListApp {
    fn render<'a>(&'a mut self, _: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        div()
            .w(px(320))
            .h(px(64))
            .flex()
            .flex_row()
            .children((0..16).map(|_| {
                div()
                    .w(px(24))
                    .h_full()
                    .flex_basis(px(24))
                    .flex_grow(1)
                    .flex_shrink(1)
            }))
    }
}

#[test]
fn flex_layout_scans_siblings_once_per_container() {
    const ITEMS: u64 = 16;

    let mut runtime = TestRuntime::default();

    let app = runtime.create(|_| FlexListApp).unwrap();

    let painter = TestPainter;

    runtime.rebuild(app).unwrap();
    runtime.reset_performance_metrics();
    runtime
        .layout(Size::new(px(320), px(64)), &painter)
        .unwrap();

    let metrics = runtime.performance_metrics();

    assert_eq!(metrics.flex_sibling_visits, ITEMS,);
    assert_eq!(metrics.flex_item_main_size_calls, ITEMS * 2,);
}

#[test]
fn flex_layout_work_is_linear_in_children() {
    const ITEMS: u64 = 16;

    let mut runtime = TestRuntime::default();

    let app = runtime.create(|_| FlexListApp).unwrap();

    let painter = TestPainter;

    runtime.rebuild(app).unwrap();
    runtime.reset_performance_metrics();
    runtime
        .layout(Size::new(px(320), px(64)), &painter)
        .unwrap();

    let metrics = runtime.performance_metrics();

    assert_eq!(metrics.flex_sibling_visits, ITEMS,);
    assert_eq!(metrics.flex_item_main_size_calls, ITEMS * 2,);
    assert_eq!(metrics.flex_base_main_size_calls, ITEMS * 4,);
}
