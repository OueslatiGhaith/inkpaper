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
