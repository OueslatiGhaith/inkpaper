use crate::{DamageRegion, Point, Rect, RefreshRegionConstraints, Size, px};

#[test]
fn refresh_constraints_preserve_none_and_full_damage() {
    let constraints =
        RefreshRegionConstraints::new(Rect::new(Point::ZERO, Size::new(px(128), px(64))))
            .with_alignment(px(8), px(4));

    assert_eq!(
        constraints.normalize(DamageRegion::none()),
        DamageRegion::none(),
    );
    assert_eq!(
        constraints.normalize(DamageRegion::full()),
        DamageRegion::full(),
    );
}

#[test]
fn refresh_constraints_clip_damage_to_display_bounds() {
    let constraints =
        RefreshRegionConstraints::new(Rect::new(Point::ZERO, Size::new(px(100), px(60))));

    let damage = DamageRegion::from_rect(Rect::new(
        Point::new(px(90), px(50)),
        Size::new(px(30), px(20)),
    ));

    let normalized = constraints.normalize(damage);

    assert_eq!(normalized.len(), 1);
    assert_eq!(
        normalized.rects()[0],
        Rect::new(Point::new(px(90), px(50)), Size::new(px(10), px(10))),
    );
}

#[test]
fn refresh_constraints_drop_damage_outside_display() {
    let constraints =
        RefreshRegionConstraints::new(Rect::new(Point::ZERO, Size::new(px(100), px(60))));

    let damage = DamageRegion::from_rect(Rect::new(
        Point::new(px(120), px(80)),
        Size::new(px(10), px(10)),
    ));

    assert_eq!(constraints.normalize(damage), DamageRegion::none());
}

#[test]
fn refresh_constraints_align_damage_outward() {
    let constraints =
        RefreshRegionConstraints::new(Rect::new(Point::ZERO, Size::new(px(96), px(64))))
            .with_alignment(px(8), px(4));

    // logical:
    // x: 11..18
    // y:  5..11
    //
    // aligned:
    // x:  8..24
    // y:  4..12
    let damage = DamageRegion::from_rect(Rect::new(
        Point::new(px(11), px(5)),
        Size::new(px(7), px(6)),
    ));

    let normalized = constraints.normalize(damage);

    assert_eq!(
        normalized.rects(),
        &[Rect::new(
            Point::new(px(8), px(4)),
            Size::new(px(16), px(8)),
        )],
    );
}

#[test]
fn refresh_constraints_align_relative_to_display_origin() {
    let constraints = RefreshRegionConstraints::new(Rect::new(
        Point::new(px(10), px(20)),
        Size::new(px(48), px(32)),
    ))
    .with_alignment(px(8), px(4));

    let damage = DamageRegion::from_rect(Rect::new(
        Point::new(px(13), px(23)),
        Size::new(px(7), px(5)),
    ));

    let normalized = constraints.normalize(damage);

    // the grid starts at (10, 20):
    // x cells:
    // 10..18
    // 18..26
    //
    // y cells:
    // 20..24
    // 24..28
    assert_eq!(
        normalized.rects(),
        &[Rect::new(
            Point::new(px(10), px(20)),
            Size::new(px(16), px(8)),
        )],
    );
}

#[test]
fn refresh_constraints_merge_regions_that_touch_after_alignment() {
    let constraints =
        RefreshRegionConstraints::new(Rect::new(Point::ZERO, Size::new(px(64), px(32))))
            .with_alignment(px(8), px(1));

    let first = Rect::new(Point::new(px(1), px(4)), Size::new(px(6), px(8)));
    let second = Rect::new(Point::new(px(9), px(4)), Size::new(px(6), px(8)));

    let damage = DamageRegion::from_rect(first).add_rect(second);
    let normalized = constraints.normalize(damage);

    // first  -> 0..8
    // second -> 8..16
    //
    // the aligned refresh windows touch, so the bounded `DamageRegion` merges
    // them into one controller window.
    assert_eq!(normalized.len(), 1);
    assert_eq!(
        normalized.rects()[0],
        Rect::new(Point::new(px(0), px(4)), Size::new(px(16), px(8))),
    );
}

#[test]
fn refresh_constraints_clip_again_after_alignment() {
    let constraints =
        RefreshRegionConstraints::new(Rect::new(Point::ZERO, Size::new(px(100), px(60))))
            .with_alignment(px(8), px(4));

    let damage = DamageRegion::from_rect(Rect::new(
        Point::new(px(95), px(57)),
        Size::new(px(4), px(2)),
    ));

    let normalized = constraints.normalize(damage);

    // alignment would expand this to:
    // x = 88..104
    // y = 56..60
    // but the physical target ends at x=100.
    assert_eq!(
        normalized.rects(),
        &[Rect::new(
            Point::new(px(88), px(56)),
            Size::new(px(12), px(4)),
        )],
    );
}
