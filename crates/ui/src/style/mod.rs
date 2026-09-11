pub use text::*;

use crate::{Color, FontId, Invalidation, Length, Pixels, px};

mod text;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Display {
    Block,
    Flex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Position {
    Static,
    Relative,
    Absolute,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum FlexDirection {
    Row,
    Column,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum AlignItems {
    Start,
    Center,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum JustifyContent {
    Start,
    Center,
    End,
    Between,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum FlexBasis {
    Auto,
    Pixels(Pixels),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
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
                $invalidation:ident {
                    $(
                        $field_name:ident: $field_ty:ty = $field_default:expr => {
                            $($field_utilities:tt)*
                        }
                    ),* $(,)?
                }
            )*

            @text {
                $(
                    $text_invalidation:ident {
                        $(
                            $text_name:ident: $text_ty:ty = $text_default:expr => {
                                $($text_utilities:tt)*
                            }
                        ),* $(,)?
                    }
                )*
            }
        }
    ) => {
        $(#[$attr])*
        pub struct Style {
            $(
                $(
                    pub $field_name: $field_ty,
                )*
            )*
            pub(crate) text: TextStyle,
        }

        impl Default for Style {
            fn default() -> Self {
                Self {
                    $(
                        $(
                            $field_name: $field_default,
                        )*
                    )*
                    text: TextStyle::default(),
                }
            }
        }

        #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
        #[cfg_attr(feature = "defmt", derive(defmt::Format))]
        pub struct TextStyle {
            $( $( pub(crate) $text_name: Option< $text_ty >, )* )*
        }

        impl TextStyle {
            pub(crate) fn between(base: Self, variant: Self) -> Self {
                Self {
                    $(
                        $(
                            $text_name: if base.$text_name != variant.$text_name {
                                variant.$text_name
                            } else {
                                None
                            },
                        )*
                    )*
                }
            }

            pub(crate) fn resolve(self, inherited: ResolvedTextStyle) -> ResolvedTextStyle {
                ResolvedTextStyle {
                    $(
                        $(
                            $text_name: self.$text_name.unwrap_or(inherited.$text_name),
                        )*
                    )*
                }
            }

            pub(crate) fn merge(self, later: Self) -> Self {
                Self {
                    $( $( $text_name: later.$text_name.or(self.$text_name), )* )*
                }
            }

            pub(crate) const fn invalidation(self) -> Invalidation {
                let mut invalidation = Invalidation::None;

                $(
                    $(
                        if self.$text_name.is_some() {
                            invalidation = invalidation.merge(
                                declare_style!(@invalidation, $text_invalidation)
                            );
                        }
                    )*
                )*

                invalidation
            }
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        #[cfg_attr(feature = "defmt", derive(defmt::Format))]
        pub struct ResolvedTextStyle {
            $( $( pub(crate) $text_name: $text_ty, )* )*
        }

        impl Default for ResolvedTextStyle {
            fn default() -> Self {
                Self {
                    $( $( $text_name: $text_default, )* )*
                }
            }
        }

        #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
        pub(crate) struct StylePatch {
            $( $( $field_name: Option< $field_ty >, )* )*
            text: TextStyle,
        }

        impl StylePatch {
            pub(crate) fn between(base: Style, variant: Style) -> Self {
                Self {
                    $(
                        $(
                            $field_name: (base.$field_name != variant.$field_name)
                                .then_some(variant.$field_name),
                        )*
                    )*
                    text: TextStyle::between(base.text, variant.text),
                }
            }

            pub(crate) fn apply(self, mut style: Style) -> Style {
                $(
                    $(
                        if let Some(value) = self.$field_name {
                            style.$field_name = value;
                        }
                    )*
                )*

                style.text = style.text.merge(self.text);

                style
            }

            pub(crate) fn merge(self, later: Self) -> Self {
                Self {
                    $(
                        $(
                            $field_name: later.$field_name.or(self.$field_name),
                        )*
                    )*
                    text: self.text.merge(later.text),
                }
            }

            pub(crate) const fn invalidation(self) -> Invalidation {
                let mut invalidation = Invalidation::None;

                $(
                    $(
                        if self.$field_name.is_some() {
                            invalidation = invalidation.merge(
                                declare_style!(@invalidation, $invalidation),
                            );
                        }
                    )*
                )*

                invalidation = invalidation.merge(self.text.invalidation());

                invalidation
            }
        }

        pub trait Styled: Sized {
            fn style_mut(&mut self) -> &mut Style;

            $(
                $(
                    declare_style!(
                        @styled_methods
                        $field_name;
                        $($field_utilities)*
                    );
                )*
            )*
        }

        pub trait TextStyled: Sized {
            fn text_style_mut(&mut self) -> &mut TextStyle;

            $(
                $(
                    declare_style!(
                        @text_styled_methods
                        $text_name;
                        $($text_utilities)*
                    );
                )*
            )*
        }

        impl<T> TextStyled for T
        where
            T: Styled,
        {
            fn text_style_mut(&mut self) -> &mut TextStyle {
                &mut self.style_mut().text
            }
        }
    };

    (@styled_methods $field_name:ident;) => {};

    (
        @styled_methods
        $field_name:ident;
        $method_name:ident(
            $( $arg_name:ident: $arg_ty:ty ),+ $(,)?
        ) => |$style:ident| $body:block;
        $($rest:tt)*
    ) => {
        fn $method_name(
            mut self,
            $( $arg_name: $arg_ty ),+
        ) -> Self {
            let $style = self.style_mut();
            $body
            self
        }

        declare_style!(@styled_methods $field_name; $($rest)*);
    };

    (
        @styled_methods
        $field_name:ident;
        $method_name:ident => |$style:ident| $body:block;
        $($rest:tt)*
    ) => {
        fn $method_name(mut self) -> Self {
            let $style = self.style_mut();
            $body
            self
        }

        declare_style!(@styled_methods $field_name; $($rest)*);
    };

    (
        @styled_methods
        $field_name:ident;
        $method_name:ident(
            $( $arg_name:ident: $arg_ty:ty ),+ $(,)?
        ) => $value:expr;
        $($rest:tt)*
    ) => {
        fn $method_name(
            mut self,
            $( $arg_name: $arg_ty ),+
        ) -> Self {
            self.style_mut().$field_name = $value;
            self
        }

        declare_style!(@styled_methods $field_name; $($rest)*);
    };

    (
        @styled_methods
        $field_name:ident;
        $method_name:ident => $value:expr;
        $($rest:tt)*
    ) => {
        fn $method_name(mut self) -> Self {
            self.style_mut().$field_name = $value;
            self
        }

        declare_style!(@styled_methods $field_name; $($rest)*);
    };

    (@text_styled_methods $field_name:ident;) => {};

    (
        @text_styled_methods
        $field_name:ident;
        $method_name:ident(
            $( $arg_name:ident: $arg_ty:ty ),+ $(,)?
        ) => |$text_style:ident| $body:block;
        $($rest:tt)*
    ) => {
        fn $method_name(
            mut self,
            $( $arg_name: $arg_ty ),+
        ) -> Self {
            let $text_style = self.text_style_mut();
            $body
            self
        }

        declare_style!(@text_styled_methods $field_name; $($rest)*);
    };

    (
        @text_styled_methods
        $field_name:ident;
        $method_name:ident => |$text_style:ident| $body:block;
        $($rest:tt)*
    ) => {
        fn $method_name(mut self) -> Self {
            let $text_style = self.text_style_mut();
            $body
            self
        }

        declare_style!(@text_styled_methods $field_name; $($rest)*);
    };

    (
        @text_styled_methods
        $field_name:ident;
        $method_name:ident(
            $( $arg_name:ident: $arg_ty:ty ),+ $(,)?
        ) => $value:expr;
        $($rest:tt)*
    ) => {
        fn $method_name(
            mut self,
            $( $arg_name: $arg_ty ),+
        ) -> Self {
            self.text_style_mut().$field_name = Some($value);
            self
        }

        declare_style!(@text_styled_methods $field_name; $($rest)*);
    };

    (
        @text_styled_methods
        $field_name:ident;
        $method_name:ident => $value:expr;
        $($rest:tt)*
    ) => {
        fn $method_name(mut self) -> Self {
            self.text_style_mut().$field_name = Some($value);
            self
        }

        declare_style!(@text_styled_methods $field_name; $($rest)*);
    };

    (@invalidation, layout) => {
        Invalidation::Layout
    };

    (@invalidation, paint) => {
        Invalidation::Paint
    };

    (@invalidation, rebuild) => {
        Invalidation::Rebuild
    };
}

inkpaper_ui_style_schema::inkpaper_style_schema!(declare_style);

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
