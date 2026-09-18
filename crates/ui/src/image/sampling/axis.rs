#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct LinearQuotient {
    quotient: u64,
    remainder: u64,
    denominator: u64,
    step_quotient: u64,
    step_remainder: u64,
}

impl LinearQuotient {
    pub(super) fn new(
        initial_numerator: u128,
        step_numerator: u64,
        denominator: u64,
    ) -> Option<Self> {
        if denominator == 0 {
            return None;
        }

        let denominator_u128 = u128::from(denominator);
        let quotient = u64::try_from(initial_numerator / denominator_u128).ok()?;
        let remainder = u64::try_from(initial_numerator % denominator_u128).ok()?;

        Some(Self {
            quotient,
            remainder,
            denominator,
            step_quotient: step_numerator / denominator,
            step_remainder: step_numerator % denominator,
        })
    }

    pub(super) const fn quotient(self) -> u64 {
        self.quotient
    }

    pub(super) const fn remainder(self) -> u64 {
        self.remainder
    }

    pub(super) fn advance(&mut self) {
        self.quotient += self.step_quotient;

        let remainder = self.remainder + self.step_remainder;

        if remainder >= self.denominator {
            self.remainder = remainder - self.denominator;
            self.quotient += 1;
        } else {
            self.remainder = remainder;
        }
    }
}
