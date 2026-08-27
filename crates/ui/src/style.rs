use crate::{
    Color, FontId, Invalidation, Length, LineHeight, Pixels, TextAlign, TextMaxLines, TextOverflow,
    TextWrap, px,
};

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
                    $( $field_name:ident: $field_ty:ty = $field_default:expr ),* $(,)?
                }
            )*
            @text {
                $(
                    $text_invalidation:ident {
                        $( $text_name:ident: $text_ty:ty = $text_default:expr ),* $(,)?
                    }
                )*
            }
        }
    ) => {
        $(#[$attr])*
        pub struct Style {
            $( $( pub $field_name: $field_ty,)* )*
            pub(crate) text: TextStyle,
        }

        impl Default for Style {
            fn default() -> Self {
                Self {
                    $( $( $field_name: $field_default, )* )*
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
                    $( $( $text_name: self.$text_name.unwrap_or(inherited.$text_name), )* )*
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
                                declare_style!(@invalidation, $invalidation)
                            );
                        }
                    )*
                )*
                invalidation = invalidation.merge(self.text.invalidation());

                invalidation
            }
        }
    };

    (@invalidation, layout) => { Invalidation::Layout };
    (@invalidation, paint) => { Invalidation::Paint };
    (@invalidation, rebuild) => { Invalidation::Rebuild };
}

declare_style! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    #[cfg_attr(feature = "defmt", derive(defmt::Format))]
    pub struct Style {
        layout {
            display: Display = Display::Block,
            position: Position = Position::Static,

            flex_direction: FlexDirection = FlexDirection::Row,
            align_items: AlignItems = AlignItems::Start,
            justify_content: JustifyContent = JustifyContent::Start,

            flex_grow: u16 = 0,
            flex_shrink: u16 = 0,
            flex_basis: FlexBasis = FlexBasis::Auto,

            width: Length = Length::Auto,
            height: Length = Length::Auto,

            min_width: Option<Pixels> = None,
            max_width: Option<Pixels> = None,
            min_height: Option<Pixels> = None,
            max_height: Option<Pixels> = None,

            padding: Edges<Pixels> = Edges::all(px(0)),
            margin: Edges<Pixels> = Edges::all(px(0)),
            gap: Pixels = px(0),

            inset: Edges<Option<Pixels>> = Edges::all(None),

            border_width: Pixels = px(0),
        }
        paint {
            background: Option<Color> = None,

            border_color: Option<Color> = None,
            border_radius: Pixels = px(0),

            clip_children: bool = false,
        }
        @text {
            layout {
                font: FontId = FontId::DEFAULT,
                line_height: LineHeight = LineHeight::Normal,
                wrap: TextWrap = TextWrap::NoWrap,
                max_lines: TextMaxLines = TextMaxLines::Unlimited,
                overflow: TextOverflow = TextOverflow::Clip,
            }
            paint {
                color: Color = Color::BLACK,
                align: TextAlign = TextAlign::Start,
            }
        }
    }
}

pub trait Styled: Sized {
    fn style_mut(&mut self) -> &mut Style;

    fn relative(mut self) -> Self {
        self.style_mut().position = Position::Relative;
        self
    }

    fn absolute(mut self) -> Self {
        self.style_mut().position = Position::Absolute;
        self
    }

    fn top(mut self, value: impl Into<Pixels>) -> Self {
        self.style_mut().inset.top = Some(value.into());
        self
    }

    fn right(mut self, value: impl Into<Pixels>) -> Self {
        self.style_mut().inset.right = Some(value.into());
        self
    }

    fn bottom(mut self, value: impl Into<Pixels>) -> Self {
        self.style_mut().inset.bottom = Some(value.into());
        self
    }

    fn left(mut self, value: impl Into<Pixels>) -> Self {
        self.style_mut().inset.left = Some(value.into());
        self
    }

    fn inset(mut self, value: impl Into<Pixels>) -> Self {
        self.style_mut().inset = Edges::all(Some(value.into()));
        self
    }

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

    fn items_start(mut self) -> Self {
        self.style_mut().align_items = AlignItems::Start;
        self
    }

    fn items_center(mut self) -> Self {
        self.style_mut().align_items = AlignItems::Center;
        self
    }

    fn items_end(mut self) -> Self {
        self.style_mut().align_items = AlignItems::End;
        self
    }

    fn justify_start(mut self) -> Self {
        self.style_mut().justify_content = JustifyContent::Start;

        self
    }

    fn justify_center(mut self) -> Self {
        self.style_mut().justify_content = JustifyContent::Center;
        self
    }

    fn justify_end(mut self) -> Self {
        self.style_mut().justify_content = JustifyContent::End;
        self
    }

    fn justify_between(mut self) -> Self {
        self.style_mut().justify_content = JustifyContent::Between;
        self
    }

    fn flex_grow(mut self, weight: u16) -> Self {
        self.style_mut().flex_grow = weight;
        self
    }

    fn flex_shrink(mut self, weight: u16) -> Self {
        self.style_mut().flex_shrink = weight;
        self
    }

    fn flex_basis(mut self, basis: Pixels) -> Self {
        self.style_mut().flex_basis = FlexBasis::Pixels(basis);
        self
    }

    fn flex_basis_auto(mut self) -> Self {
        self.style_mut().flex_basis = FlexBasis::Auto;
        self
    }

    fn flex_1(mut self) -> Self {
        self.style_mut().flex_grow = 1;
        self.style_mut().flex_shrink = 1;
        self.style_mut().flex_basis = FlexBasis::Pixels(px(0));
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

    fn min_w(mut self, value: impl Into<Pixels>) -> Self {
        self.style_mut().min_width = Some(value.into());
        self
    }

    fn max_w(mut self, value: impl Into<Pixels>) -> Self {
        self.style_mut().max_width = Some(value.into());
        self
    }

    fn min_h(mut self, value: impl Into<Pixels>) -> Self {
        self.style_mut().min_height = Some(value.into());
        self
    }

    fn max_h(mut self, value: impl Into<Pixels>) -> Self {
        self.style_mut().max_height = Some(value.into());
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

    fn px(mut self, padding: impl Into<Pixels>) -> Self {
        let padding = padding.into();
        self.style_mut().padding.right = padding;
        self.style_mut().padding.left = padding;
        self
    }

    fn py(mut self, padding: impl Into<Pixels>) -> Self {
        let padding = padding.into();
        self.style_mut().padding.top = padding;
        self.style_mut().padding.bottom = padding;
        self
    }

    fn m(mut self, margin: impl Into<Pixels>) -> Self {
        self.style_mut().margin = Edges::all(margin.into());
        self
    }

    fn mt(mut self, margin: impl Into<Pixels>) -> Self {
        self.style_mut().margin.top = margin.into();
        self
    }

    fn mr(mut self, margin: impl Into<Pixels>) -> Self {
        self.style_mut().margin.right = margin.into();
        self
    }

    fn mb(mut self, margin: impl Into<Pixels>) -> Self {
        self.style_mut().margin.bottom = margin.into();
        self
    }

    fn ml(mut self, margin: impl Into<Pixels>) -> Self {
        self.style_mut().margin.left = margin.into();
        self
    }

    fn mx(mut self, margin: impl Into<Pixels>) -> Self {
        let margin = margin.into();
        self.style_mut().margin.right = margin;
        self.style_mut().margin.left = margin;
        self
    }

    fn my(mut self, margin: impl Into<Pixels>) -> Self {
        let margin = margin.into();
        self.style_mut().margin.top = margin;
        self.style_mut().margin.bottom = margin;
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

    fn border(mut self, width: impl Into<Pixels>) -> Self {
        self.style_mut().border_width = width.into();
        self
    }

    fn border_color(mut self, color: Color) -> Self {
        self.style_mut().border_color = Some(color);
        self
    }

    fn rounded(mut self, radius: impl Into<Pixels>) -> Self {
        self.style_mut().border_radius = radius.into();
        self
    }

    fn overflow_hidden(mut self) -> Self {
        self.style_mut().clip_children = true;
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
