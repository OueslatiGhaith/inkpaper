use std::convert::Infallible;

use inkpaper_ui::{BoxPaint, Painter, ResolvedTextStyle, TextMeasurer, prelude::*};

const VIEWPORT: Size = Size::new(px(320), px(240));

const ROW_COUNTS: [usize; 4] = [8, 32, 128, 512];

type BenchRuntime = Runtime<4096, 4, 4096, 16, 2048, 32768, 1024>;

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
        let mut painter = NullCanvasPainter;

        draw(
            Rect::new(Point::ZERO, Size::new(bounds.width(), bounds.height())),
            &mut painter,
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

fn main() {
    println!(
        "rows,entity_renders,nodes_mounted,measure_calls,text_measurements,flex_base_calls,\
flex_item_calls,flex_sibling_visits,nodes_laid_out,visual_nodes_visited,visible_nodes,\
nodes_painted"
    );

    for rows in ROW_COUNTS {
        let mut runtime = BenchRuntime::default();

        let app = runtime.create(move |_| ListApp { rows }).unwrap();

        let painter = NullPainter;

        runtime.rebuild(app).unwrap();
        runtime.layout(VIEWPORT, &painter).unwrap();
        runtime.reset_performance_metrics();

        runtime.rebuild(app).unwrap();

        let rebuild = runtime.performance_metrics();

        runtime.reset_performance_metrics();

        runtime.layout(VIEWPORT, &painter).unwrap();

        let layout = runtime.performance_metrics();

        runtime.reset_performance_metrics();

        runtime.paint(&mut NullPainter).unwrap();

        let paint = runtime.performance_metrics();

        println!(
            "{},{},{},{},{},{},{},{},{},{},{},{}",
            rows,
            rebuild.entity_render_calls,
            rebuild.nodes_mounted,
            layout.measure_node_calls,
            layout.text_measurements,
            layout.flex_base_main_size_calls,
            layout.flex_item_main_size_calls,
            layout.flex_sibling_visits,
            layout.nodes_laid_out,
            paint.visual_nodes_visited,
            paint.visible_nodes,
            paint.nodes_painted,
        );
    }
}
