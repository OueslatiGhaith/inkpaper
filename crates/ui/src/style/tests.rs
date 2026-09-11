use super::*;

#[derive(Default)]
struct TestStyled {
    style: Style,
}

impl Styled for TestStyled {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

#[test]
fn generated_layout_methods_preserve_existing_behavior() {
    let styled = TestStyled::default()
        .relative()
        .absolute()
        .flex()
        .flex_col()
        .flex_row()
        .items_start()
        .items_center()
        .items_end()
        .justify_start()
        .justify_center()
        .justify_end()
        .justify_between()
        .flex_grow(3)
        .flex_shrink(2)
        .flex_basis(px(7))
        .flex_basis_auto()
        .w(px(100))
        .w_full()
        .h(px(50))
        .h_full()
        .min_w(px(10))
        .max_w(px(200))
        .min_h(px(20))
        .max_h(px(300))
        .gap(px(6))
        .border(px(2));

    assert_eq!(styled.style.position, Position::Absolute);
    assert_eq!(styled.style.display, Display::Flex);
    assert_eq!(styled.style.flex_direction, FlexDirection::Row);
    assert_eq!(styled.style.align_items, AlignItems::End);
    assert_eq!(styled.style.justify_content, JustifyContent::Between);
    assert_eq!(styled.style.flex_grow, 3);
    assert_eq!(styled.style.flex_shrink, 2);
    assert_eq!(styled.style.flex_basis, FlexBasis::Auto);
    assert_eq!(styled.style.width, Length::Fill);
    assert_eq!(styled.style.height, Length::Fill);
    assert_eq!(styled.style.min_width, Some(px(10)));
    assert_eq!(styled.style.max_width, Some(px(200)));
    assert_eq!(styled.style.min_height, Some(px(20)));
    assert_eq!(styled.style.max_height, Some(px(300)));
    assert_eq!(styled.style.gap, px(6));
    assert_eq!(styled.style.border_width, px(2));
}

#[test]
fn generated_flex_one_preserves_existing_behavior() {
    let styled = TestStyled::default()
        .flex_grow(8)
        .flex_shrink(4)
        .flex_basis(px(32))
        .flex_1();

    assert_eq!(styled.style.flex_grow, 1);
    assert_eq!(styled.style.flex_shrink, 1);
    assert_eq!(styled.style.flex_basis, FlexBasis::Pixels(px(0)));
}

#[test]
fn generated_edge_methods_preserve_existing_behavior() {
    let styled = TestStyled::default()
        .inset(px(20))
        .top(px(1))
        .right(px(2))
        .bottom(px(3))
        .left(px(4))
        .p(px(10))
        .px(px(11))
        .py(px(12))
        .pt(px(5))
        .pr(px(6))
        .pb(px(7))
        .pl(px(8))
        .m(px(20))
        .mx(px(21))
        .my(px(22))
        .mt(px(13))
        .mr(px(14))
        .mb(px(15))
        .ml(px(16));

    assert_eq!(styled.style.inset.top, Some(px(1)));
    assert_eq!(styled.style.inset.right, Some(px(2)));
    assert_eq!(styled.style.inset.bottom, Some(px(3)));
    assert_eq!(styled.style.inset.left, Some(px(4)));

    assert_eq!(styled.style.padding.top, px(5));
    assert_eq!(styled.style.padding.right, px(6));
    assert_eq!(styled.style.padding.bottom, px(7));
    assert_eq!(styled.style.padding.left, px(8));

    assert_eq!(styled.style.margin.top, px(13));
    assert_eq!(styled.style.margin.right, px(14));
    assert_eq!(styled.style.margin.bottom, px(15));
    assert_eq!(styled.style.margin.left, px(16));
}

#[test]
fn generated_paint_methods_preserve_existing_behavior() {
    let styled = TestStyled::default()
        .bg(Color::RED)
        .border_color(Color::BLUE)
        .rounded(px(9))
        .overflow_hidden();

    assert_eq!(styled.style.background, Some(Color::RED));
    assert_eq!(styled.style.border_color, Some(Color::BLUE));
    assert_eq!(styled.style.border_radius, px(9));
    assert!(styled.style.clip_children);
}

#[test]
fn generated_text_methods_preserve_existing_behavior() {
    let font = FontId::new(7);

    let styled = TestStyled::default()
        .font(font)
        .font_size(px(0))
        .text_color(Color::GREEN)
        .line_height(px(20))
        .line_height_normal()
        .text_wrap(TextWrap::NoWrap)
        .no_wrap()
        .wrap()
        .text_align(TextAlign::Start)
        .text_end()
        .text_center()
        .text_start()
        .max_lines(3)
        .unlimited_lines()
        .text_overflow(TextOverflow::Clip)
        .text_clip()
        .text_ellipsis();

    assert_eq!(styled.style.text.font, Some(font));
    assert_eq!(styled.style.text.font_size, Some(px(1)));
    assert_eq!(styled.style.text.color, Some(Color::GREEN));
    assert_eq!(styled.style.text.line_height, Some(LineHeight::Normal));
    assert_eq!(styled.style.text.wrap, Some(TextWrap::Word));
    assert_eq!(styled.style.text.align, Some(TextAlign::Start));
    assert_eq!(styled.style.text.max_lines, Some(TextMaxLines::Unlimited));
    assert_eq!(styled.style.text.overflow, Some(TextOverflow::Ellipsis));
}

#[test]
fn generated_line_height_preserves_non_negative_conversion() {
    let styled = TestStyled::default().line_height(px(-5));

    assert_eq!(
        styled.style.text.line_height,
        Some(LineHeight::Pixels(px(0)))
    );
}
