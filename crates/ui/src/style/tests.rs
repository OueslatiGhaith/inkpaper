use crate::FontWeight;

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
fn generated_text_methods_preserve_existing_behavior() {
    let family = FontFamilyId::new(3);

    let styled = TestStyled::default()
        .font_family(family)
        .font_weight(FontWeight::BOLD)
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

    assert_eq!(styled.style.text.font_family, Some(family));
    assert_eq!(styled.style.text.font_weight, Some(FontWeight::BOLD));
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

#[test]
fn text_darkness_threshold_matches_its_curve() {
    // a painter moving its threshold must ink exactly the pixels whose painted
    // coverage reaches it
    for percent in [0, 1, 50, 60, 75, 99, 100, 101, 140, 180, 200, 255] {
        let darkness = TextDarkness::percent(percent);

        for threshold in 0..=u8::MAX {
            let min = darkness.min_coverage_for(threshold);

            for coverage in 0..=u8::MAX {
                assert_eq!(
                    darkness.apply(coverage) >= threshold,
                    coverage >= min,
                    "{percent}% at {threshold}, coverage {coverage}",
                );
            }
        }
    }
}

#[test]
fn text_darkness_keeps_empty_and_full_coverage() {
    for percent in [0, 1, 50, 75, 100, 140, 180, 255] {
        let darkness = TextDarkness::percent(percent);

        assert_eq!(darkness.apply(0), 0, "{percent}%");
        assert_eq!(darkness.apply(u8::MAX), u8::MAX, "{percent}%");

        for coverage in 1..=u8::MAX {
            assert!(
                darkness.apply(coverage) >= darkness.apply(coverage - 1),
                "{percent}% falls at {coverage}",
            );
        }
    }
}

#[test]
fn text_darkness_moves_where_coverage_paints_half_ink() {
    assert_eq!(TextDarkness::NORMAL.apply(77), 77);

    // lighter needs more coverage for half ink, darker less
    assert_eq!(TextDarkness::percent(75).apply(171), 128);
    assert_eq!(TextDarkness::percent(200).apply(64), 128);

    // the reader's levels keep their binary thresholds
    let thresholds =
        [75, 100, 140, 180].map(|percent| TextDarkness::percent(percent).min_coverage_for(112));
    assert_eq!(thresholds, [150, 112, 80, 63]);
}
