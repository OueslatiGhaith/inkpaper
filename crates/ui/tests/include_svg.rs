use inkpaper_ui::prelude::*;

const ICON: SvgSource = include_svg!("tests/fixtures/basic.svg");

#[test]
fn include_svg_builds_static_svg_source() {
    assert_eq!(ICON.size(), Size::new(px(24), px(24)));
    assert_eq!(ICON.view_box(), SvgViewBox::new(0.0, 0.0, 24.0, 24.0));
    assert_eq!(ICON.paths().len(), 2);

    let line = ICON.paths()[0];

    assert_eq!(
        line.path.commands(),
        &[
            PathCommand::MoveTo(VectorPoint::new(3.0, 12.0)),
            PathCommand::LineTo(VectorPoint::new(21.0, 12.0)),
        ],
    );

    assert_eq!(
        line.stroke,
        Some(
            SvgStroke::new(2.0, SvgPaint::CurrentColor)
                .with_cap(StrokeCap::Round)
                .with_join(StrokeJoin::Round),
        ),
    );

    assert!(line.fill.is_none());

    let circle = ICON.paths()[1];

    assert_eq!(circle.path.commands().len(), 6);
}
