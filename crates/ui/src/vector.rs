use crate::{Color, Pixels};

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct VectorPoint {
    pub x: f32,
    pub y: f32,
}

impl VectorPoint {
    pub const ZERO: Self = Self::new(0.0, 0.0);

    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PathCommand {
    MoveTo(VectorPoint),
    LineTo(VectorPoint),
    QuadraticTo {
        control: VectorPoint,
        to: VectorPoint,
    },
    CubicTo {
        control_1: VectorPoint,
        control_2: VectorPoint,
        to: VectorPoint,
    },
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VectorPath<'a> {
    commands: &'a [PathCommand],
}

impl<'a> VectorPath<'a> {
    pub const fn new(commands: &'a [PathCommand]) -> Self {
        Self { commands }
    }

    pub const fn commands(self) -> &'a [PathCommand] {
        self.commands
    }

    pub const fn is_empty(self) -> bool {
        self.commands.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AffineTransform {
    m11: f32,
    m12: f32,
    m21: f32,
    m22: f32,
    translate_x: f32,
    translate_y: f32,
}

impl AffineTransform {
    pub const IDENTITY: Self = Self::new(1.0, 0.0, 0.0, 1.0, 0.0, 0.0);

    pub const fn new(
        m11: f32,
        m12: f32,
        m21: f32,
        m22: f32,
        translate_x: f32,
        translate_y: f32,
    ) -> Self {
        Self {
            m11,
            m12,
            m21,
            m22,
            translate_x,
            translate_y,
        }
    }

    pub const fn scale(x: f32, y: f32) -> Self {
        Self::new(x, 0.0, 0.0, y, 0.0, 0.0)
    }

    pub const fn translation(x: f32, y: f32) -> Self {
        Self::new(1.0, 0.0, 0.0, 1.0, x, y)
    }

    pub const fn scale_translate(
        scale_x: f32,
        scale_y: f32,
        translate_x: f32,
        translate_y: f32,
    ) -> Self {
        Self::new(scale_x, 0.0, 0.0, scale_y, translate_x, translate_y)
    }

    pub fn map_point(self, point: VectorPoint) -> VectorPoint {
        VectorPoint::new(
            self.m11 * point.x + self.m21 * point.y + self.translate_x,
            self.m12 * point.x + self.m22 * point.y + self.translate_y,
        )
    }
}

impl Default for AffineTransform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum FillRule {
    #[default]
    NonZero,
    EvenOdd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PathFill {
    pub color: Color,
    pub rule: FillRule,
}

impl PathFill {
    pub const fn new(color: Color) -> Self {
        Self {
            color,
            rule: FillRule::NonZero,
        }
    }

    pub const fn with_rule(mut self, rule: FillRule) -> Self {
        self.rule = rule;
        self
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum StrokeCap {
    #[default]
    Butt,
    Round,
    Square,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum StrokeJoin {
    #[default]
    Miter,
    Round,
    Bevel,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PathStroke {
    pub width: Pixels,
    pub color: Color,
    pub cap: StrokeCap,
    pub join: StrokeJoin,
    pub miter_limit: f32,
}

impl PathStroke {
    pub const fn new(width: Pixels, color: Color) -> Self {
        Self {
            width,
            color,
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
