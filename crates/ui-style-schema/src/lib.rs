#![no_std]

pub mod tailwind;

#[macro_export]
macro_rules! inkpaper_style_schema {
    ($declare:ident) => {
        $declare! {
            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            #[cfg_attr(feature = "defmt", derive(defmt::Format))]
            pub struct Style {
                layout {
                    display: Display = Display::Block => {
                        flex => Display::Flex;
                    },

                    position: Position = Position::Static => {
                        relative => Position::Relative;
                        absolute => Position::Absolute;
                    },

                    flex_direction: FlexDirection = FlexDirection::Row => {
                        flex_row => FlexDirection::Row;
                        flex_col => FlexDirection::Column;
                    },

                    align_items: AlignItems = AlignItems::Start => {
                        items_start => AlignItems::Start;
                        items_center => AlignItems::Center;
                        items_end => AlignItems::End;
                    },

                    justify_content: JustifyContent = JustifyContent::Start => {
                        justify_start => JustifyContent::Start;
                        justify_center => JustifyContent::Center;
                        justify_end => JustifyContent::End;
                        justify_between => JustifyContent::Between;
                    },

                    flex_grow: u16 = 0 => {
                        @tailwind(Integer, class = "grow")
                        flex_grow(weight: u16) => weight;
                    },

                    flex_shrink: u16 = 0 => {
                        @tailwind(Integer, class = "shrink")
                        flex_shrink(weight: u16) => weight;
                    },

                    flex_basis: FlexBasis = FlexBasis::Auto => {
                        @tailwind(Spacing, class = "basis")
                        flex_basis(basis: Pixels) => FlexBasis::Pixels(basis);

                        @tailwind(class = "basis-auto")
                        flex_basis_auto => FlexBasis::Auto;

                        flex_1 => |style| {
                            style.flex_grow = 1;
                            style.flex_shrink = 1;
                            style.flex_basis = FlexBasis::Pixels(px(0));
                        };
                    },

                    width: Length = Length::Auto => {
                        @tailwind(Spacing)
                        w(width: impl Into<Length>) => width.into();

                        w_full => Length::Fill;
                    },

                    height: Length = Length::Auto => {
                        @tailwind(Spacing)
                        h(height: impl Into<Length>) => height.into();

                        h_full => Length::Fill;
                    },

                    min_width: Option<Pixels> = None => {
                        @tailwind(Spacing)
                        min_w(value: impl Into<Pixels>) => Some(value.into());
                    },

                    max_width: Option<Pixels> = None => {
                        @tailwind(Spacing)
                        max_w(value: impl Into<Pixels>) => Some(value.into());
                    },

                    min_height: Option<Pixels> = None => {
                        @tailwind(Spacing)
                        min_h(value: impl Into<Pixels>) => Some(value.into());
                    },

                    max_height: Option<Pixels> = None => {
                        @tailwind(Spacing)
                        max_h(value: impl Into<Pixels>) => Some(value.into());
                    },

                    padding: Edges<Pixels> = Edges::all(px(0)) => {
                        @tailwind(Spacing)
                        p(padding: impl Into<Pixels>) => Edges::all(padding.into());

                        @tailwind(Spacing)
                        pt(padding: impl Into<Pixels>) => |style| {
                            style.padding.top = padding.into();
                        };

                        @tailwind(Spacing)
                        pr(padding: impl Into<Pixels>) => |style| {
                            style.padding.right = padding.into();
                        };

                        @tailwind(Spacing)
                        pb(padding: impl Into<Pixels>) => |style| {
                            style.padding.bottom = padding.into();
                        };

                        @tailwind(Spacing)
                        pl(padding: impl Into<Pixels>) => |style| {
                            style.padding.left = padding.into();
                        };

                        @tailwind(Spacing)
                        px(padding: impl Into<Pixels>) => |style| {
                            let padding = padding.into();
                            style.padding.right = padding;
                            style.padding.left = padding;
                        };

                        @tailwind(Spacing)
                        py(padding: impl Into<Pixels>) => |style| {
                            let padding = padding.into();
                            style.padding.top = padding;
                            style.padding.bottom = padding;
                        };
                    },

                    margin: Edges<Pixels> = Edges::all(px(0)) => {
                        @tailwind(Spacing)
                        m(margin: impl Into<Pixels>) => Edges::all(margin.into());

                        @tailwind(Spacing)
                        mt(margin: impl Into<Pixels>) => |style| {
                            style.margin.top = margin.into();
                        };

                        @tailwind(Spacing)
                        mr(margin: impl Into<Pixels>) => |style| {
                            style.margin.right = margin.into();
                        };

                        @tailwind(Spacing)
                        mb(margin: impl Into<Pixels>) => |style| {
                            style.margin.bottom = margin.into();
                        };

                        @tailwind(Spacing)
                        ml(margin: impl Into<Pixels>) => |style| {
                            style.margin.left = margin.into();
                        };

                        @tailwind(Spacing)
                        mx(margin: impl Into<Pixels>) => |style| {
                            let margin = margin.into();
                            style.margin.right = margin;
                            style.margin.left = margin;
                        };

                        @tailwind(Spacing)
                        my(margin: impl Into<Pixels>) => |style| {
                            let margin = margin.into();
                            style.margin.top = margin;
                            style.margin.bottom = margin;
                        };
                    },

                    gap: Pixels = px(0) => {
                        @tailwind(Spacing)
                        gap(gap: impl Into<Pixels>) => gap.into();
                    },

                    inset: Edges<Option<Pixels>> = Edges::all(None) => {
                        @tailwind(Spacing)
                        inset(value: impl Into<Pixels>) => Edges::all(Some(value.into()));

                        @tailwind(Spacing)
                        top(value: impl Into<Pixels>) => |style| {
                            style.inset.top = Some(value.into());
                        };

                        @tailwind(Spacing)
                        right(value: impl Into<Pixels>) => |style| {
                            style.inset.right = Some(value.into());
                        };

                        @tailwind(Spacing)
                        bottom(value: impl Into<Pixels>) => |style| {
                            style.inset.bottom = Some(value.into());
                        };

                        @tailwind(Spacing)
                        left(value: impl Into<Pixels>) => |style| {
                            style.inset.left = Some(value.into());
                        };
                    },

                    border_width: Pixels = px(0) => {
                        @tailwind(BorderWidth)
                        border(width: impl Into<Pixels>) => width.into();
                    },
                }

                paint {
                    background: Option<Color> = None => {
                        @tailwind(Color)
                        bg(color: Color) => Some(color);
                    },

                    border_color: Option<Color> = None => {
                        border_color(color: Color) => Some(color);
                    },

                    border_radius: Pixels = px(0) => {
                        @tailwind(Radius)
                        rounded(radius: impl Into<Pixels>) => radius.into();
                    },

                    clip_children: bool = false => {
                        overflow_hidden => true;
                    },
                }

                @text {
                    layout {
                        font: FontId = FontId::DEFAULT => {
                            font(font: FontId) => font;
                        },

                        font_size: Pixels = px(16) => {
                            @tailwind(FontSize, class = "text")
                            font_size(size: impl Into<Pixels>) => size.into().max(px(1));
                        },

                        line_height: LineHeight = LineHeight::Normal => {
                            @tailwind(LineHeight, class = "leading")
                            line_height(line_height: impl Into<LineHeight>) => line_height.into();

                            line_height_normal => LineHeight::Normal;
                        },

                        wrap: TextWrap = TextWrap::NoWrap => {
                            text_wrap(wrap: TextWrap) => wrap;
                            wrap => TextWrap::Word;
                            no_wrap => TextWrap::NoWrap;
                        },

                        max_lines: TextMaxLines = TextMaxLines::Unlimited => {
                            max_lines(lines: u16) => TextMaxLines::Limited(lines);
                            unlimited_lines => TextMaxLines::Unlimited;
                        },

                        overflow: TextOverflow = TextOverflow::Clip => {
                            text_overflow(overflow: TextOverflow) => overflow;
                            text_ellipsis => TextOverflow::Ellipsis;
                            text_clip => TextOverflow::Clip;
                        },
                    }

                    paint {
                        color: Color = Color::BLACK => {
                            text_color(color: Color) => color;
                        },

                        align: TextAlign = TextAlign::Start => {
                            text_align(align: TextAlign) => align;
                            text_start => TextAlign::Start;
                            text_center => TextAlign::Center;
                            text_end => TextAlign::End;
                        },
                    }
                }
            }
        }
    };
}
