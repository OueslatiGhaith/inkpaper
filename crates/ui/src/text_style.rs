use crate::{Color, Pixels, Styled, TextStyle};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FontId(u16);

impl FontId {
    pub const DEFAULT: Self = Self(0);

    pub const fn new(value: u16) -> Self {
        Self(value)
    }

    pub(crate) const fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    #[default]
    Start,
    Center,
    End,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum TextWrap {
    #[default]
    NoWrap,
    Word,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum LineHeight {
    #[default]
    Normal,
    Pixels(Pixels),
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
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

pub trait TextStyled: Sized {
    fn text_style_mut(&mut self) -> &mut TextStyle;

    fn font(mut self, font: FontId) -> Self {
        self.text_style_mut().font = Some(font);
        self
    }

    fn text_color(mut self, color: Color) -> Self {
        self.text_style_mut().color = Some(color);
        self
    }

    fn line_height(mut self, line_height: impl Into<LineHeight>) -> Self {
        self.text_style_mut().line_height = Some(line_height.into());
        self
    }

    fn line_height_normal(mut self) -> Self {
        self.text_style_mut().line_height = Some(LineHeight::Normal);
        self
    }

    fn text_wrap(mut self, wrap: TextWrap) -> Self {
        self.text_style_mut().wrap = Some(wrap);
        self
    }

    fn wrap(self) -> Self {
        self.text_wrap(TextWrap::Word)
    }

    fn no_wrap(self) -> Self {
        self.text_wrap(TextWrap::NoWrap)
    }

    fn text_align(mut self, align: TextAlign) -> Self {
        self.text_style_mut().align = Some(align);
        self
    }

    fn text_start(self) -> Self {
        self.text_align(TextAlign::Start)
    }

    fn text_center(self) -> Self {
        self.text_align(TextAlign::Center)
    }

    fn text_end(self) -> Self {
        self.text_align(TextAlign::End)
    }

    fn max_lines(mut self, lines: u16) -> Self {
        self.text_style_mut().max_lines = Some(TextMaxLines::Limited(lines));
        self
    }

    fn unlimited_lines(mut self) -> Self {
        self.text_style_mut().max_lines = Some(TextMaxLines::Unlimited);
        self
    }

    fn text_overflow(mut self, overflow: TextOverflow) -> Self {
        self.text_style_mut().overflow = Some(overflow);
        self
    }

    fn text_ellipsis(self) -> Self {
        self.text_overflow(TextOverflow::Ellipsis)
    }

    fn text_clip(self) -> Self {
        self.text_overflow(TextOverflow::Clip)
    }
}

impl<T> TextStyled for T
where
    T: Styled,
{
    fn text_style_mut(&mut self) -> &mut TextStyle {
        &mut self.style_mut().text
    }
}
