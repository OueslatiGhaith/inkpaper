use crate::{Color, FillRule, Size, StrokeCap, StrokeJoin, VectorPath};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SvgViewBox {
    pub min_x: f32,
    pub min_y: f32,
    pub width: f32,
    pub height: f32,
}

impl SvgViewBox {
    pub const fn new(min_x: f32, min_y: f32, width: f32, height: f32) -> Self {
        Self {
            min_x,
            min_y,
            width,
            height,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SvgPaint {
    Color(Color),
    CurrentColor,
}

impl SvgPaint {
    pub(crate) const fn resolve(self, current_color: Color) -> Color {
        match self {
            Self::Color(color) => color,
            Self::CurrentColor => current_color,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SvgFill {
    pub paint: SvgPaint,
    pub rule: FillRule,
}

impl SvgFill {
    pub const fn new(paint: SvgPaint) -> Self {
        Self {
            paint,
            rule: FillRule::NonZero,
        }
    }

    pub const fn with_rule(mut self, rule: FillRule) -> Self {
        self.rule = rule;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SvgStroke {
    pub paint: SvgPaint,
    pub width: f32,
    pub cap: StrokeCap,
    pub join: StrokeJoin,
    pub miter_limit: f32,
}

impl SvgStroke {
    pub const fn new(width: f32, paint: SvgPaint) -> Self {
        Self {
            paint,
            width,
            cap: StrokeCap::Butt,
            join: StrokeJoin::Miter,
            miter_limit: 4.0,
        }
    }

    pub const fn with_cap(mut self, cap: StrokeCap) -> Self {
        self.cap = cap;
        self
    }

    pub const fn with_join(mut self, join: StrokeJoin) -> Self {
        self.join = join;
        self
    }

    pub const fn with_miter_limit(mut self, miter_limit: f32) -> Self {
        self.miter_limit = miter_limit;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SvgPath {
    pub path: VectorPath<'static>,
    pub fill: Option<SvgFill>,
    pub stroke: Option<SvgStroke>,
}

impl SvgPath {
    pub const fn new(path: VectorPath<'static>) -> Self {
        Self {
            path,
            fill: None,
            stroke: None,
        }
    }

    pub const fn with_fill(mut self, fill: SvgFill) -> Self {
        self.fill = Some(fill);
        self
    }

    pub const fn with_stroke(mut self, stroke: SvgStroke) -> Self {
        self.stroke = Some(stroke);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SvgSource {
    size: Size,
    view_box: SvgViewBox,
    paths: &'static [SvgPath],
}

impl SvgSource {
    pub const fn new(size: Size, view_box: SvgViewBox, paths: &'static [SvgPath]) -> Self {
        Self {
            size,
            view_box,
            paths,
        }
    }

    pub const fn size(self) -> Size {
        self.size
    }

    pub const fn view_box(self) -> SvgViewBox {
        self.view_box
    }

    pub const fn paths(self) -> &'static [SvgPath] {
        self.paths
    }

    pub fn has_paint(self) -> bool {
        self.paths
            .iter()
            .any(|path| path.fill.is_some() || path.stroke.is_some())
    }
}
