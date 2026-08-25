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
