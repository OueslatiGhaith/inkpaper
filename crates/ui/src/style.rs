use crate::{Color, Invalidation, Length, Pixels, px};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

macro_rules! declare_style {
    (
        $(#[$attr:meta])*
        pub struct Style {
            $(
                #[$invalidation:ident]
                $field_vis:vis $field_name:ident: $field_ty:ty
            ),* $(,)?
        }
    ) => {
        $(#[$attr])*
        pub struct Style {
            $( $field_vis $field_name: $field_ty ),*
        }

        #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
        pub(crate) struct StylePatch {
            $( $field_vis $field_name: Option< $field_ty > ),*
        }

        impl StylePatch {
            pub(crate) fn between(base: Style, variant: Style) -> Self {
                Self {
                    $(
                        $field_name: (base.$field_name != variant.$field_name)
                            .then_some(variant.$field_name)
                    ),*
                }
            }

            pub(crate) fn apply(self, mut style: Style) -> Style {
                $(
                    if let Some(value) = self.$field_name {
                        style.$field_name = value;
                    }
                )*

                style
            }

            pub(crate) fn merge(self, later: Self) -> Self {
                Self {
                    $($field_name: later.$field_name.or(self.$field_name)),*
                }
            }

            pub(crate) const fn invalidation(self) -> Invalidation {
                $(
                    declare_style!(@check_layout, $invalidation, self.$field_name);
                )*
                $(
                    declare_style!(@check_paint, $invalidation, self.$field_name);
                )*

                Invalidation::None
            }
        }
    };

    (@check_layout, layout, $field:expr) => {
        if $field.is_some() {
            return Invalidation::Layout;
        }
    };
    (@check_layout, paint, $field:expr) => {};
    (@check_paint, paint, $field:expr) => {
        if $field.is_some() {
            return Invalidation::Paint;
        }
    };
    (@check_paint, layout, $field:expr) => {};
}

declare_style! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Style {
        #[layout]
        pub display: Display,
        #[layout]
        pub flex_direction: FlexDirection,
        #[layout]
        pub width: Length,
        #[layout]
        pub height: Length,
        #[layout]
        pub padding: Edges<Pixels>,
        #[layout]
        pub gap: Pixels,

        #[paint]
        pub background: Option<Color>,
    }
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
            background: None,
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

    fn bg(mut self, color: Color) -> Self {
        self.style_mut().background = Some(color);
        self
    }
}

#[derive(Debug, Clone, Copy)]
pub struct InteractionStyle {
    style: Style,
}

impl InteractionStyle {
    pub(crate) const fn new(style: Style) -> Self {
        Self { style }
    }

    pub(crate) const fn into_style(self) -> Style {
        self.style
    }
}

impl Styled for InteractionStyle {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}
