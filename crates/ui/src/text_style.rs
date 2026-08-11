use crate::{Color, Pixels, Styled};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FontId(u16);

impl FontId {
    pub const DEFAULT: Self = Self(0);

    pub const fn new(value: u16) -> Self {
        Self(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextStyle {
    pub font: FontId,
    pub color: Color,
    pub line_height: Option<Pixels>,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            font: FontId::DEFAULT,
            color: Color::BLACK,
            line_height: None,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct TextStylePatch {
    pub(crate) font: Option<FontId>,
    pub(crate) color: Option<Color>,
    pub(crate) line_height: Option<Pixels>,
}

impl TextStylePatch {
    pub const fn font(self) -> Option<FontId> {
        self.font
    }

    pub const fn color(self) -> Option<Color> {
        self.color
    }

    pub const fn line_height(self) -> Option<Pixels> {
        self.line_height
    }

    pub fn resolve(self, inherited: TextStyle) -> TextStyle {
        TextStyle {
            font: match self.font {
                Some(font) => font,
                None => inherited.font,
            },
            color: match self.color {
                Some(color) => color,
                None => inherited.color,
            },
            line_height: match self.line_height {
                Some(line_height) => Some(line_height),
                None => inherited.line_height,
            },
        }
    }

    pub fn merge(self, later: Self) -> Self {
        Self {
            font: later.font.or(self.font),
            color: later.color.or(self.color),
            line_height: later.line_height.or(self.line_height),
        }
    }
}

pub trait TextStyled: Sized {
    fn font(self, font: FontId) -> Self;
    fn text_color(self, color: Color) -> Self;
    fn line_height(self, line_height: Pixels) -> Self;
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
}
