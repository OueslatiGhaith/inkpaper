use crate::{Pixels, px};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Point {
    pub x: Pixels,
    pub y: Pixels,
}

impl Point {
    pub const ZERO: Self = Self {
        x: Pixels::ZERO,
        y: Pixels::ZERO,
    };

    pub const fn new(x: Pixels, y: Pixels) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    pub width: Pixels,
    pub height: Pixels,
}

impl Size {
    pub const ZERO: Self = Self {
        width: Pixels::ZERO,
        height: Pixels::ZERO,
    };

    pub const fn new(width: Pixels, height: Pixels) -> Self {
        Self { width, height }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub origin: Point,
    pub size: Size,
}

impl Rect {
    pub const fn new(origin: Point, size: Size) -> Self {
        Self { origin, size }
    }

    pub const fn x(self) -> Pixels {
        self.origin.x
    }

    pub const fn y(self) -> Pixels {
        self.origin.y
    }

    pub const fn width(self) -> Pixels {
        self.size.width
    }

    pub const fn height(self) -> Pixels {
        self.size.height
    }

    pub const fn right(self) -> Pixels {
        px(self.origin.x.0 + self.size.width.0)
    }

    pub const fn bottom(self) -> Pixels {
        px(self.origin.y.0 + self.size.height.0)
    }

    pub fn contains(self, point: Point) -> bool {
        let width = self.size.width.0;
        let height = self.size.height.0;
        if width <= 0 || height <= 0 {
            return false;
        }

        let left = self.origin.x.0;
        let top = self.origin.y.0;
        let right = left.saturating_add(width);
        let bottom = top.saturating_add(height);

        point.x.0 >= left && point.x.0 < right && point.y.0 >= top && point.y.0 < bottom
    }
}
