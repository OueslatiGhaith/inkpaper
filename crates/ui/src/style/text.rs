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

/// how dark glyphs paint: a gain on their coverage, in percent. At 100 they
/// paint as rasterized; above it their edges darken and, where coverage is
/// thresholded, their stems thicken
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct TextDarkness(u8);

impl TextDarkness {
    pub const NORMAL: Self = Self(100);

    pub const fn percent(percent: u8) -> Self {
        Self(percent)
    }

    pub const fn get(self) -> u8 {
        self.0
    }

    /// `coverage` after the gain
    pub const fn apply(self, coverage: u8) -> u8 {
        if self.0 == 100 {
            return coverage;
        }

        let scaled = coverage as u32 * self.0 as u32 / 100;

        if scaled > u8::MAX as u32 {
            u8::MAX
        } else {
            scaled as u8
        }
    }

    /// the least rasterized coverage that reaches `threshold` after the gain,
    /// or `None` when none does
    pub const fn min_coverage_for(self, threshold: u8) -> Option<u8> {
        if self.0 == 100 || threshold == 0 {
            return Some(threshold);
        }

        if self.0 == 0 {
            return None;
        }

        let gain = self.0 as u32;
        let min = (threshold as u32 * 100).div_ceil(gain);

        if min > u8::MAX as u32 {
            None
        } else {
            Some(min as u8)
        }
    }
}

impl Default for TextDarkness {
    fn default() -> Self {
        Self::NORMAL
    }
}
