use crate::Pixels;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum TextAlign {
    #[default]
    Start,
    Center,
    End,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum TextWrap {
    #[default]
    NoWrap,
    Word,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum LineHeight {
    #[default]
    Normal,
    Pixels(Pixels),
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum TextMaxLines {
    #[default]
    Unlimited,
    Limited(u16),
}

impl TextMaxLines {
    pub(crate) const fn limit(self) -> Option<usize> {
        match self {
            Self::Unlimited => None,
            Self::Limited(lines) => {
                let lines = if lines == 0 { 1 } else { lines };
                Some(lines as usize)
            }
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum TextOverflow {
    #[default]
    Clip,
    Ellipsis,
}

impl From<Pixels> for LineHeight {
    fn from(value: Pixels) -> Self {
        Self::Pixels(value.non_negative())
    }
}

/// how dark glyphs paint, in percent of their weight as rasterized. It moves
/// the coverage that paints as half ink, so above 100 edges darken and stems
/// thicken, and below it they lighten and thin. Empty pixels stay empty and
/// fully covered ones fully inked, so stems stay solid on painters that draw
/// gray edges, and only where a thresholding painter cuts moves
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct TextDarkness(u8);

impl TextDarkness {
    pub const NORMAL: Self = Self(100);

    /// coverage that paints as half ink
    const HALF: u32 = 128;

    pub const fn percent(percent: u8) -> Self {
        Self(percent)
    }

    pub const fn get(self) -> u8 {
        self.0
    }

    /// the rasterized coverage that paints as half ink
    const fn pivot(self) -> u32 {
        let percent = if self.0 == 0 { 1 } else { self.0 as u32 };
        let pivot = (Self::HALF * 100 + percent / 2) / percent;

        if pivot == 0 {
            1
        } else if pivot > u8::MAX as u32 {
            u8::MAX as u32
        } else {
            pivot
        }
    }

    /// `coverage` as this darkness paints it: two straight segments through
    /// the pivot, keeping 0 and 255
    #[expect(
        clippy::cast_possible_truncation,
        reason = "both curve halves map 0..=255 into 0..=255"
    )]
    pub const fn apply(self, coverage: u8) -> u8 {
        if self.0 == 100 {
            return coverage;
        }

        let pivot = self.pivot();
        let coverage = coverage as u32;
        let max = u8::MAX as u32;

        let painted = if coverage < pivot {
            coverage * Self::HALF / pivot
        } else if pivot == max {
            max
        } else {
            Self::HALF + (coverage - pivot) * (max - Self::HALF) / (max - pivot)
        };

        painted as u8
    }

    /// the least rasterized coverage that paints at least `threshold`
    pub const fn min_coverage_for(self, threshold: u8) -> u8 {
        if self.0 == 100 {
            return threshold;
        }

        // `apply` only grows with coverage and keeps 255, so some coverage
        // always reaches the threshold
        let mut low = 0u8;
        let mut high = u8::MAX;

        while low < high {
            let middle = low + (high - low) / 2;

            if self.apply(middle) >= threshold {
                high = middle;
            } else {
                low = middle + 1;
            }
        }

        low
    }
}

impl Default for TextDarkness {
    fn default() -> Self {
        Self::NORMAL
    }
}
