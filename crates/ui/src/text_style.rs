use crate::{Color, Pixels, Style, Styled};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextStyle {
    pub font: FontId,
    pub color: Color,
    pub line_height: Option<Pixels>,
    pub align: TextAlign,
    pub wrap: TextWrap,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            font: FontId::DEFAULT,
            color: Color::BLACK,
            line_height: None,
            align: TextAlign::Start,
            wrap: TextWrap::NoWrap,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct TextStylePatch {
    pub(crate) font: Option<FontId>,
    pub(crate) color: Option<Color>,
    pub(crate) line_height: Option<Pixels>,
    pub(crate) align: Option<TextAlign>,
    pub(crate) wrap: Option<TextWrap>,
}

impl TextStylePatch {
    pub(crate) const fn from_style(style: Style) -> Self {
        Self {
            font: style.font,
            color: style.text_color,
            line_height: style.line_height,
            align: style.text_align,
            wrap: style.text_wrap,
        }
    }

    pub const fn font(self) -> Option<FontId> {
        self.font
    }

    pub const fn color(self) -> Option<Color> {
        self.color
    }

    pub const fn line_height(self) -> Option<Pixels> {
        self.line_height
    }

    pub const fn align(self) -> Option<TextAlign> {
        self.align
    }

    pub const fn wrap(self) -> Option<TextWrap> {
        self.wrap
    }

    pub fn resolve(self, inherited: TextStyle) -> TextStyle {
        TextStyle {
            font: self.font.unwrap_or(inherited.font),
            color: self.color.unwrap_or(inherited.color),
            line_height: self.line_height.or(inherited.line_height),
            align: self.align.unwrap_or(inherited.align),
            wrap: self.wrap.unwrap_or(inherited.wrap),
        }
    }

    pub fn merge(self, later: Self) -> Self {
        Self {
            font: later.font.or(self.font),
            color: later.color.or(self.color),
            line_height: later.line_height.or(self.line_height),
            align: later.align.or(self.align),
            wrap: later.wrap.or(self.wrap),
        }
    }
}

pub trait TextStyled: Sized {
    fn font(self, font: FontId) -> Self;
    fn text_color(self, color: Color) -> Self;
    fn line_height(self, line_height: Pixels) -> Self;
    fn text_align(self, align: TextAlign) -> Self;
    fn text_wrap(self, wrap: TextWrap) -> Self;

    fn text_start(self) -> Self {
        self.text_align(TextAlign::Start)
    }

    fn text_center(self) -> Self {
        self.text_align(TextAlign::Center)
    }

    fn text_end(self) -> Self {
        self.text_align(TextAlign::End)
    }

    fn wrap(self) -> Self {
        self.text_wrap(TextWrap::Word)
    }

    fn no_wrap(self) -> Self {
        self.text_wrap(TextWrap::NoWrap)
    }
}

impl<T> TextStyled for T
where
    T: Styled,
{
    fn font(mut self, font: FontId) -> Self {
        self.style_mut().font = Some(font);
        self
    }

    fn text_color(mut self, color: Color) -> Self {
        self.style_mut().text_color = Some(color);
        self
    }

    fn line_height(mut self, line_height: Pixels) -> Self {
        self.style_mut().line_height = Some(line_height.non_negative());
        self
    }

    fn text_align(mut self, align: TextAlign) -> Self {
        self.style_mut().text_align = Some(align);
        self
    }

    fn text_wrap(mut self, wrap: TextWrap) -> Self {
        self.style_mut().text_wrap = Some(wrap);
        self
    }
}
