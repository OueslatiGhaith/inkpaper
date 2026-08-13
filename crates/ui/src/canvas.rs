use crate::{
    Color, Element, MountCx, MountError, NodeId, Pixels, Point, Rect, Size, callback::CallbackId,
};

pub trait CanvasPainter {
    fn fill_rect(&mut self, rect: Rect, color: Color);
    fn stroke_rect(&mut self, rect: Rect, width: Pixels, color: Color);
    fn line(&mut self, start: Point, end: Point, width: Pixels, color: Color);
    fn fill_circle(&mut self, center: Point, radius: Pixels, color: Color);
    fn stroke_circle(&mut self, center: Point, radius: Pixels, width: Pixels, color: Color);
}

pub type CanvasDrawFn = fn(bounds: Rect, painter: &mut dyn CanvasPainter);

#[derive(Debug, Clone, Copy)]
pub(crate) enum CanvasDraw {
    Static(CanvasDrawFn),
    Entity(CallbackId),
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CanvasStyle {
    pub(crate) width: Option<Pixels>,
    pub(crate) height: Option<Pixels>,
}

#[derive(Debug, Clone, Copy)]
pub struct Canvas {
    draw: CanvasDraw,
    style: CanvasStyle,
}

impl Canvas {
    pub const fn new(draw: CanvasDrawFn) -> Self {
        Self {
            draw: CanvasDraw::Static(draw),
            style: CanvasStyle {
                width: None,
                height: None,
            },
        }
    }

    pub(crate) const fn from_entity_callback(callback: CallbackId) -> Self {
        Self {
            draw: CanvasDraw::Entity(callback),
            style: CanvasStyle {
                width: None,
                height: None,
            },
        }
    }

    pub fn w(mut self, width: Pixels) -> Self {
        self.style.width = Some(width);
        self
    }

    pub fn h(mut self, height: Pixels) -> Self {
        self.style.height = Some(height);
        self
    }

    pub fn size(mut self, size: Size) -> Self {
        self.style.width = Some(size.width);
        self.style.height = Some(size.height);
        self
    }
}

impl Element for Canvas {
    fn mount(self, cx: &mut MountCx<'_>) -> Result<NodeId, MountError> {
        cx.push_canvas(self.draw, self.style)
    }
}

pub const fn canvas(draw: CanvasDrawFn) -> Canvas {
    Canvas::new(draw)
}
