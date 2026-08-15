use core::ops::{Add, AddAssign, Sub, SubAssign};

use crate::Pixels;

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

impl Add<Offset> for Point {
    type Output = Self;

    fn add(self, rhs: Offset) -> Self::Output {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl Sub<Offset> for Point {
    type Output = Self;

    fn sub(self, rhs: Offset) -> Self::Output {
        Self::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl Sub<Point> for Point {
    type Output = Offset;

    fn sub(self, rhs: Point) -> Self::Output {
        Offset::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl AddAssign<Offset> for Point {
    fn add_assign(&mut self, rhs: Offset) {
        self.x += rhs.x;
        self.y += rhs.y;
    }
}

impl SubAssign<Offset> for Point {
    fn sub_assign(&mut self, rhs: Offset) {
        self.x -= rhs.x;
        self.y -= rhs.y;
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
        self.origin.x + self.size.width
    }

    pub fn bottom(self) -> Pixels {
        self.origin.y + self.size.height
    }

    pub fn contains(self, point: Point) -> bool {
        if self.size.width.is_non_positive() || self.size.height.is_non_positive() {
            return false;
        }

        point.x >= self.origin.x
            && point.x < self.right()
            && point.y >= self.origin.y
            && point.y < self.bottom()
    }

    pub fn translated(self, delta: Offset) -> Self {
        Self::new(self.origin + delta, self.size)
    }

    pub fn intersection(self, other: Self) -> Option<Self> {
        let left = self.x().max(other.x());
        let top = self.y().max(other.y());
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());

        if right <= left || bottom <= top {
            return None;
        }

        Some(Self::new(
            Point::new(left, top),
            Size::new(right - left, bottom - top),
        ))
    }

    pub fn inset(self, amount: Pixels) -> Self {
        let amount = amount.non_negative();
        let doubled = amount * 2;

        Self::new(
            self.origin + Offset::new(amount, amount),
            Size::new(
                (self.size.width - doubled).non_negative(),
                (self.size.height - doubled).non_negative(),
            ),
        )
    }

    pub fn has_area(&self) -> bool {
        self.width().is_positive() && self.height().is_positive()
    }

    pub fn union(self, other: Self) -> Self {
        if !self.has_area() {
            return other;
        }
        if !other.has_area() {
            return self;
        }

        let left = self.x().min(other.x());
        let top = self.y().min(other.y());
        let right = self.right().max(other.right());
        let bottom = self.bottom().max(other.bottom());

        Self::new(Point::new(left, top), Size::new(right - left, bottom - top))
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Offset {
    pub x: Pixels,
    pub y: Pixels,
}

impl Offset {
    pub const ZERO: Self = Self::new(Pixels::ZERO, Pixels::ZERO);

    pub const fn new(x: Pixels, y: Pixels) -> Self {
        Self { x, y }
    }
}

impl Add for Offset {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl Sub for Offset {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl AddAssign for Offset {
    fn add_assign(&mut self, rhs: Self) {
        self.x += rhs.x;
        self.y += rhs.y;
    }
}

impl SubAssign for Offset {
    fn sub_assign(&mut self, rhs: Self) {
        self.x -= rhs.x;
        self.y -= rhs.y;
    }
}
