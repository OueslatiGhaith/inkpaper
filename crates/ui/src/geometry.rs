use core::ops::{Add, AddAssign, Div, Mul, Sub, SubAssign};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Pixels(i32);

pub const fn px(value: i32) -> Pixels {
    Pixels(value)
}

impl Pixels {
    pub const ZERO: Self = px(0);
    pub const MAX: Self = px(i32::MAX);

    pub const fn get(self) -> i32 {
        self.0
    }

    pub const fn max(self, other: Self) -> Self {
        if self.0 >= other.0 { self } else { other }
    }

    pub const fn min(self, other: Self) -> Self {
        if self.0 <= other.0 { self } else { other }
    }

    pub const fn clamp(self, minimum: Self, maximum: Self) -> Self {
        self.max(minimum).min(maximum)
    }

    pub const fn saturating_add(self, other: Self) -> Self {
        Self(self.0.saturating_add(other.0))
    }

    pub const fn saturating_sub(self, other: Self) -> Self {
        Self(self.0.saturating_sub(other.0))
    }

    pub const fn saturating_mul(self, multiplier: i32) -> Self {
        Self(self.0.saturating_mul(multiplier))
    }

    pub const fn non_negative(self) -> Self {
        Self(if self.0 < 0 { 0 } else { self.0 })
    }

    pub const fn is_positive(self) -> bool {
        self.0 > 0
    }

    pub const fn is_non_positive(self) -> bool {
        self.0 <= 0
    }

    pub(crate) fn scale_ratio_floor(&self, numerator: Pixels, denominator: Pixels) -> Self {
        let value = i64::from(self.0.max(0));
        let numerator = i64::from(numerator.0.max(0));
        let denominator = i64::from(denominator.0.max(0));

        if value == 0 || numerator == 0 || denominator == 0 {
            return px(0);
        }

        let result = value.saturating_mul(numerator) / denominator;

        Self(i32::try_from(result).unwrap_or(i32::MAX))
    }

    pub(crate) fn scale_ratio_ceil(self, numerator: Self, denominator: Self) -> Self {
        let value = i64::from(self.0.max(0));
        let numerator = i64::from(numerator.0.max(0));
        let denominator = i64::from(denominator.0.max(0));

        if value == 0 || numerator == 0 || denominator == 0 {
            return Self::ZERO;
        }

        let product = value.saturating_mul(numerator);
        let result = product.saturating_add(denominator.saturating_sub(1)) / denominator;

        Self(i32::try_from(result).unwrap_or(i32::MAX))
    }
}

impl Add for Pixels {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        self.saturating_add(rhs)
    }
}

impl Sub for Pixels {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        self.saturating_sub(rhs)
    }
}

impl AddAssign for Pixels {
    fn add_assign(&mut self, rhs: Self) {
        *self = self.saturating_add(rhs);
    }
}

impl SubAssign for Pixels {
    fn sub_assign(&mut self, rhs: Self) {
        *self = self.saturating_sub(rhs);
    }
}

impl Mul<i32> for Pixels {
    type Output = Self;

    fn mul(self, rhs: i32) -> Self::Output {
        self.saturating_mul(rhs)
    }
}

impl Div<i32> for Pixels {
    type Output = Self;

    fn div(self, rhs: i32) -> Self::Output {
        if rhs == 0 {
            return Self::ZERO;
        }
        Self(self.0 / rhs)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Length {
    Auto,
    Pixels(Pixels),
    Fill,
}

impl From<Pixels> for Length {
    fn from(value: Pixels) -> Self {
        Self::Pixels(value)
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
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
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
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
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
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
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
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
