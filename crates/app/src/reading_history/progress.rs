const MAX_BASIS_POINTS: u16 = 10_000;
const BASIS_POINTS_PER_PERCENT: u16 = 100;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BookProgress(u16);

impl BookProgress {
    pub const ZERO: Self = Self(0);
    pub const COMPLETE: Self = Self(MAX_BASIS_POINTS);

    pub const fn from_basis_points(value: u16) -> Self {
        if value > MAX_BASIS_POINTS {
            Self::COMPLETE
        } else {
            Self(value)
        }
    }

    pub const fn basis_points(self) -> u16 {
        self.0
    }

    pub const fn percent(self) -> u8 {
        // `self.0` is always <= 10_000,
        // therefore this is always <= 100.
        (self.0 / BASIS_POINTS_PER_PERCENT) as u8
    }

    pub(crate) fn from_ratio(numerator: u128, denominator: u128) -> Self {
        if denominator == 0 {
            return Self::ZERO;
        }

        let numerator = numerator.min(denominator);

        let basis_points = numerator.saturating_mul(u128::from(MAX_BASIS_POINTS)) / denominator;

        Self::from_basis_points(u16::try_from(basis_points).unwrap_or(MAX_BASIS_POINTS))
    }
}
