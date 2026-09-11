use crate::{
    CanvasDraw, CanvasPainter, ImagePaint, ImageSource, PaintCx, PaintReport, PaintSink, Rect,
    ResolvedTextStyle, ResourcePainter, paint::PaintRuntime,
};

#[cfg(test)]
#[path = "canvas_tests.rs"]
mod tests;

struct CanvasPaintSink<'a, R: ?Sized, P: ResourcePainter<R>> {
    painter: &'a mut P,
    resources: &'a mut R,
    report: &'a mut PaintReport,
    error: Option<P::Error>,
}

impl<'a, R: ?Sized, P: ResourcePainter<R>> PaintSink for CanvasPaintSink<'a, R, P> {
    fn text(&mut self, bounds: Rect, clip: Rect, text: &str, style: ResolvedTextStyle) {
        if self.error.is_some() {
            return;
        }

        match self
            .painter
            .draw_text(self.resources, text, bounds, style, Some(clip))
        {
            Ok(()) => self.report.mark_text(),
            Err(error) => self.error = Some(error),
        }
    }

    fn image(&mut self, bounds: Rect, clip: Rect, source: ImageSource, paint: ImagePaint) {
        if self.error.is_some() {
            return;
        }

        match self
            .painter
            .draw_image(self.resources, source, bounds, paint, Some(clip))
        {
            Ok(()) => self.report.mark_image(paint.color_mode),
            Err(error) => self.error = Some(error),
        }
    }

    fn box_paint(&mut self, bounds: Rect, clip: Rect, paint: super::BoxPaint) {
        if self.error.is_some() {
            return;
        }

        match self.painter.draw_box(bounds, paint, Some(clip)) {
            Ok(()) if paint.background.is_some() || paint.border.is_some() => {
                self.report.mark_graphics()
            }
            Ok(()) => {}
            Err(error) => self.error = Some(error),
        }
    }

    fn shapes(
        &mut self,
        bounds: Rect,
        clip: Rect,
        draw: &mut dyn FnMut(Rect, &mut dyn CanvasPainter),
    ) {
        if self.error.is_some() {
            return;
        }

        match self.painter.draw_canvas(bounds, Some(clip), draw) {
            Ok(()) => self.report.mark_graphics(),
            Err(error) => self.error = Some(error),
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn paint_canvas<R: ?Sized, P: ResourcePainter<R>>(
    draw: CanvasDraw,
    bounds: Rect,
    clip: Option<Rect>,
    style: ResolvedTextStyle,
    runtime: Option<PaintRuntime<'_>>,
    resources: &mut R,
    report: &mut PaintReport,
    painter: &mut P,
) -> Result<(), P::Error> {
    if !bounds.has_area() || clip.is_some_and(|clip| bounds.intersection(clip).is_none()) {
        return Ok(());
    }

    let mut sink = CanvasPaintSink {
        painter,
        resources,
        report,
        error: None,
    };
    let mut cx = PaintCx::new(&mut sink, bounds, clip, style);

    match draw {
        CanvasDraw::Static(draw) => draw(&mut cx),
        CanvasDraw::Entity(callback) => {
            let runtime = runtime.expect("entity canvas requires runtime callback context");

            runtime
                .callbacks
                .invoke_canvas(callback, &mut cx, runtime.entities)
                .expect("canvas callback must be live and its entity readable");
        }
    }

    match sink.error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}
