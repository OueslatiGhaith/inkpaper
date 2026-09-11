use crate::{
    BoxPaint, CanvasPainter, ImagePaint, ImageSource, Offset, Point, Rect, ResolvedTextStyle, Text,
};

pub(crate) trait PaintSink {
    fn text(&mut self, bounds: Rect, clip: Rect, text: &str, style: ResolvedTextStyle);
    fn image(&mut self, bounds: Rect, clip: Rect, source: ImageSource, paint: ImagePaint);
    fn box_paint(&mut self, bounds: Rect, clip: Rect, paint: BoxPaint);
    fn shapes(
        &mut self,
        bounds: Rect,
        clip: Rect,
        draw: &mut dyn FnMut(Rect, &mut dyn CanvasPainter),
    );
}

/// drawing coordinates are local to the canvas. Clips can only be narrowed.
/// Callbacks may be skipped or repeated for damage rectangles. Only read state
/// and draw. Backend errors stop subsequent drawing and are returned by
/// the runtime's paint call
pub struct PaintCx<'a> {
    sink: &'a mut dyn PaintSink,
    bounds: Rect,
    clip: Option<Rect>,
    text_style: ResolvedTextStyle,
}

impl<'a> PaintCx<'a> {
    pub(crate) fn new(
        sink: &'a mut dyn PaintSink,
        bounds: Rect,
        clip: Option<Rect>,
        text_style: ResolvedTextStyle,
    ) -> Self {
        let clip = match clip {
            Some(clip) => bounds.intersection(clip),
            None => bounds.intersection(bounds),
        };
        Self {
            sink,
            bounds,
            clip,
            text_style,
        }
    }

    /// the actual layout bounds, expressed in canvas-local coordinates
    pub fn bounds(&self) -> Rect {
        Rect::new(Point::ZERO, self.bounds.size)
    }

    /// restricts drawing for this call without changing the coordinate origin
    pub fn with_clip(&mut self, clip: Rect, draw: impl FnOnce(&mut PaintCx<'_>)) {
        let clip = self.destination(clip).map(|(_, clip)| clip);
        let mut child = PaintCx {
            sink: &mut *self.sink,
            bounds: self.bounds,
            clip,
            text_style: self.text_style,
        };

        draw(&mut child)
    }

    /// paints text inside explicit bounds, inheriting the canvas's text style
    pub fn draw_text(&mut self, bounds: Rect, text: Text<'_>) {
        if text.text.is_empty() {
            return;
        }

        if let Some((bounds, clip)) = self.destination(bounds) {
            self.sink
                .text(bounds, clip, text.text, text.style.resolve(self.text_style));
        }
    }

    pub fn draw_image(&mut self, bounds: Rect, source: ImageSource, paint: ImagePaint) {
        if let Some((bounds, clip)) = self.destination(bounds) {
            self.sink.image(bounds, clip, source, paint);
        }
    }

    pub fn draw_box(&mut self, bounds: Rect, paint: BoxPaint) {
        if let Some((bounds, clip)) = self.destination(bounds) {
            self.sink.box_paint(bounds, clip, paint);
        }
    }

    /// draws shapes relative to the supplied rectangle's origin
    pub fn draw_shapes(
        &mut self,
        bounds: Rect,
        mut draw: impl FnMut(Rect, &mut dyn CanvasPainter),
    ) {
        if let Some((bounds, clip)) = self.destination(bounds) {
            self.sink.shapes(bounds, clip, &mut draw);
        }
    }

    fn destination(&self, bounds: Rect) -> Option<(Rect, Rect)> {
        let bounds = bounds.translated(Offset::new(self.bounds.x(), self.bounds.y()));

        Some((bounds, self.clip?.intersection(bounds)?))
    }
}
