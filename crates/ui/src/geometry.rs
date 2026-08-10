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

    pub fn right(self) -> Pixels {
        px(self.origin.x.0.saturating_add(self.size.width.0))
    }

    pub fn bottom(self) -> Pixels {
        px(self.origin.y.0.saturating_add(self.size.height.0))
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

    pub fn translated(self, delta: Point) -> Self {
        Self::new(
            Point::new(
                px(self.origin.x.0.saturating_add(delta.x.0)),
                px(self.origin.y.0.saturating_add(delta.y.0)),
            ),
            self.size,
        )
    }

    pub fn intersection(self, other: Self) -> Option<Self> {
        let left = self.x().0.max(other.x().0);
        let top = self.y().0.max(other.y().0);
        let right = self.right().0.min(other.right().0);
        let bottom = self.bottom().0.min(other.bottom().0);

        if right <= left || bottom <= top {
            return None;
        }

        Some(Self::new(
            Point::new(px(left), px(top)),
            Size::new(px(right - left), px(bottom - top)),
        ))
    }

    pub fn inset(self, amount: Pixels) -> Self {
        let amount = amount.0.max(0);
        let doubled = amount.saturating_mul(2);

        Self::new(
            Point::new(
                px(self.origin.x.0.saturating_add(amount)),
                px(self.origin.y.0.saturating_add(amount)),
            ),
            Size::new(
                px(self.size.width.0.saturating_sub(doubled).max(0)),
                px(self.size.height.0.saturating_sub(doubled).max(0)),
            ),
        )
    }
}
