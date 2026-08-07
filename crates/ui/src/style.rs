use crate::{Length, Pixels, px};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Display {
    Block,
    Flex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlexDirection {
    Row,
    Column,
}

#[derive(Debug, Clone, Copy)]
pub struct Edges<T> {
    pub top: T,
    pub right: T,
    pub bottom: T,
    pub left: T,
}

impl<T: Copy> Edges<T> {
    pub const fn all(value: T) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }
}

pub struct Style {
    pub display: Display,
    pub flex_direction: FlexDirection,

    pub width: Length,
    pub height: Length,

    pub padding: Edges<Pixels>,
    pub gap: Pixels,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            display: Display::Block,
            flex_direction: FlexDirection::Row,
            width: Length::Auto,
            height: Length::Auto,
            padding: Edges::all(px(0)),
            gap: px(0),
        }
    }
}

pub trait Styled: Sized {
    fn style_mut(&mut self) -> &mut Style;

    fn flex(mut self) -> Self {
        self.style_mut().display = Display::Flex;
        self
    }

    fn flex_col(mut self) -> Self {
        self.style_mut().flex_direction = FlexDirection::Column;
        self
    }

    fn flex_row(mut self) -> Self {
        self.style_mut().flex_direction = FlexDirection::Row;
        self
    }

    fn w(mut self, width: impl Into<Length>) -> Self {
        self.style_mut().width = width.into();
        self
    }

    fn w_full(mut self) -> Self {
        self.style_mut().width = Length::Fill;
        self
    }

    fn h(mut self, height: impl Into<Length>) -> Self {
        self.style_mut().height = height.into();
        self
    }

    fn h_full(mut self) -> Self {
        self.style_mut().height = Length::Fill;
        self
    }

    fn p(mut self, padding: impl Into<Pixels>) -> Self {
        self.style_mut().padding = Edges::all(padding.into());
        self
    }

    fn pt(mut self, padding: impl Into<Pixels>) -> Self {
        self.style_mut().padding.top = padding.into();
        self
    }

    fn pr(mut self, padding: impl Into<Pixels>) -> Self {
        self.style_mut().padding.right = padding.into();
        self
    }

    fn pb(mut self, padding: impl Into<Pixels>) -> Self {
        self.style_mut().padding.bottom = padding.into();
        self
    }

    fn pl(mut self, padding: impl Into<Pixels>) -> Self {
        self.style_mut().padding.left = padding.into();
        self
    }

    fn gap(mut self, gap: impl Into<Pixels>) -> Self {
        self.style_mut().gap = gap.into();
        self
    }
}
