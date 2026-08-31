
use core::convert::Infallible;

use crate::{DamageRegion, DisplayPresenter, Point, Rect, RefreshRegionConstraints, Size, px};

const EMPTY_RECT: Rect = Rect::new(Point::ZERO, Size::ZERO);

struct RecordingPresenter {
    full_calls: u8,
    partial_calls: u8,
    last_regions: [Rect; 4],
    last_region_count: usize,
}

impl RecordingPresenter {
    fn new() -> Self {
        Self {
            full_calls: 0,
            partial_calls: 0,
            last_regions: [EMPTY_RECT; 4],
            last_region_count: 0,
        }
    }
}

impl DisplayPresenter for RecordingPresenter {
    type Error = Infallible;

    fn present_full(&mut self) -> Result<(), Self::Error> {
        self.full_calls = self.full_calls.saturating_add(1);

        Ok(())
    }

    fn present_partial(&mut self, regions: &[Rect]) -> Result<(), Self::Error> {
        self.partial_calls = self.partial_calls.saturating_add(1);

        self.last_region_count = regions.len();

        for (destination, source) in self.last_regions.iter_mut().zip(regions.iter().copied()) {
            *destination = source;
        }

        Ok(())
    }
}

#[test]
fn display_presenter_ignores_empty_damage() {
    let mut presenter = RecordingPresenter::new();

    presenter.present_damage(DamageRegion::none()).unwrap();

    assert_eq!(presenter.full_calls, 0,);
    assert_eq!(presenter.partial_calls, 0,);
    assert_eq!(presenter.last_region_count, 0,);
}

#[test]
fn display_presenter_routes_full_damage_to_full_refresh() {
    let mut presenter = RecordingPresenter::new();

    presenter.present_damage(DamageRegion::full()).unwrap();

    assert_eq!(presenter.full_calls, 1,);
    assert_eq!(presenter.partial_calls, 0,);
    assert_eq!(presenter.last_region_count, 0,);
}

#[test]
fn display_presenter_routes_partial_damage_regions() {
    let first = Rect::new(Point::new(px(4), px(8)), Size::new(px(20), px(10)));
    let second = Rect::new(Point::new(px(80), px(50)), Size::new(px(15), px(12)));

    let damage = DamageRegion::from_rect(first).add_rect(second);

    let mut presenter = RecordingPresenter::new();

    presenter.present_damage(damage).unwrap();

    assert_eq!(presenter.full_calls, 0,);
    assert_eq!(presenter.partial_calls, 1,);
    assert_eq!(presenter.last_region_count, 2,);
    assert_eq!(presenter.last_regions[0], first,);
    assert_eq!(presenter.last_regions[1], second,);
}

struct FullRefreshOnlyPresenter {
    refreshes: u8,
}

impl DisplayPresenter for FullRefreshOnlyPresenter {
    type Error = Infallible;

    fn present_full(&mut self) -> Result<(), Self::Error> {
        self.refreshes = self.refreshes.saturating_add(1);

        Ok(())
    }

    fn present_partial(&mut self, _regions: &[Rect]) -> Result<(), Self::Error> {
        // a display without hardware partial refresh support can
        // conservatively promote partial damage to a full refresh.
        self.present_full()
    }
}

#[test]
fn display_presenter_can_promote_partial_damage_to_full_refresh() {
    let damage = DamageRegion::from_rect(Rect::new(
        Point::new(px(10), px(10)),
        Size::new(px(20), px(20)),
    ));

    let mut presenter = FullRefreshOnlyPresenter { refreshes: 0 };

    presenter.present_damage(damage).unwrap();

    assert_eq!(presenter.refreshes, 1,);
}

#[test]
fn refresh_constraints_preserve_none_and_full_damage() {
    let constraints =
        RefreshRegionConstraints::new(Rect::new(Point::ZERO, Size::new(px(128), px(64))))
            .with_alignment(px(8), px(4));

    assert_eq!(
        constraints.normalize(DamageRegion::none(),),
        DamageRegion::none(),
    );
    assert_eq!(
        constraints.normalize(DamageRegion::full(),),
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

    assert_eq!(normalized.len(), 1,);
    assert_eq!(
        normalized.rects()[0],
        Rect::new(Point::new(px(90), px(50),), Size::new(px(10), px(10),),),
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

    assert_eq!(constraints.normalize(damage,), DamageRegion::none(),);
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
            Point::new(px(8), px(4),),
            Size::new(px(16), px(8),),
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
            Point::new(px(10), px(20),),
            Size::new(px(16), px(8),),
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
    assert_eq!(normalized.len(), 1,);
    assert_eq!(
        normalized.rects()[0],
        Rect::new(Point::new(px(0), px(4),), Size::new(px(16), px(8),),),
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
            Point::new(px(88), px(56),),
            Size::new(px(12), px(4),),
        )],
    );
}

#[test]
fn display_presenter_can_normalize_damage_before_partial_refresh() {
    let constraints =
        RefreshRegionConstraints::new(Rect::new(Point::ZERO, Size::new(px(64), px(32))))
            .with_alignment(px(8), px(1));

    let damage = DamageRegion::from_rect(Rect::new(
        Point::new(px(11), px(5)),
        Size::new(px(4), px(6)),
    ));

    let mut presenter = RecordingPresenter::new();

    presenter
        .present_constrained_damage(damage, constraints)
        .unwrap();

    assert_eq!(presenter.full_calls, 0,);
    assert_eq!(presenter.partial_calls, 1,);
    assert_eq!(presenter.last_region_count, 1,);
    assert_eq!(
        presenter.last_regions[0],
        Rect::new(Point::new(px(8), px(5),), Size::new(px(8), px(6),),),
    );
}
