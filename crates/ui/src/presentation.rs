use crate::{DamageRegion, Pixels, Point, Rect, Size, px};

/// presents framebuffer changes to a physical or logical display.
///
/// rendering and presentation are deliberately separate:
/// - the renderer updates framebuffer pixels covered by [`DamageRegion`]
/// - the presenter decides how those changed pixels are transferred to the display
///
/// a presenter may conservatively expand or merge partial regions. It may also fallback
/// to a full refresh when partial refresh is not supported or when the display requires one.
pub trait DisplayPresenter {
    type Error;

    /// present the complete framebuffer
    fn present_full(&mut self) -> Result<(), Self::Error>;

    /// present one or more non-empty damaged regions.
    ///
    /// `present_damage()` only calls this method for partial damage, so `regions` is
    /// guaranteed to be non-empty.
    ///
    /// implementations may merge, align, expand, or otherwise normalize these regions
    /// as required by the display controller
    fn present_partial(&mut self, regions: &[Rect]) -> Result<(), Self::Error>;

    /// present framebuffer changes described by `damage`:
    /// - empty damage performs no display operation
    /// - full damage selects `present_full()`
    /// - partial damage selects `present_partial()`
    fn present_damage(&mut self, damage: DamageRegion) -> Result<(), Self::Error> {
        if damage.is_none() {
            return Ok(());
        }
        if damage.is_full() {
            return self.present_full();
        }

        let regions = damage.rects();
        debug_assert!(
            !regions.is_empty(),
            "partial damage must contain at least one region"
        );

        self.present_partial(regions)
    }

    fn present_constrained_damage(
        &mut self,
        damage: DamageRegion,
        constraints: RefreshRegionConstraints,
    ) -> Result<(), Self::Error> {
        self.present_damage(constraints.normalize(damage))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefreshRegionConstraints {
    bounds: Rect,
    horizontal_alignment: Pixels,
    vertical_alignment: Pixels,
}

impl RefreshRegionConstraints {
    pub const fn new(bounds: Rect) -> Self {
        Self {
            bounds,
            horizontal_alignment: px(1),
            vertical_alignment: px(1),
        }
    }

    pub const fn with_alignment(mut self, horizontal: Pixels, vertical: Pixels) -> Self {
        assert!(
            horizontal.is_positive(),
            "horizontal refresh alignment must be positive"
        );
        assert!(
            vertical.is_positive(),
            "vertical refresh alignment must be positive"
        );

        self.horizontal_alignment = horizontal;
        self.vertical_alignment = vertical;

        self
    }

    pub const fn bounds(self) -> Rect {
        self.bounds
    }

    pub const fn horizontal_alignment(self) -> Pixels {
        self.horizontal_alignment
    }

    pub const fn vertical_alignment(self) -> Pixels {
        self.vertical_alignment
    }

    pub fn normalize(self, damage: DamageRegion) -> DamageRegion {
        if damage.is_none() {
            return DamageRegion::none();
        }

        // full damage is semantic, not merely geometric.
        // do not convert it into a rectangle coverting `bounds` because that would turn
        // a requested full refresh into a partial refresh covering the whole screen
        if damage.is_full() {
            return DamageRegion::full();
        }

        if !self.bounds.has_area() {
            return DamageRegion::none();
        }

        let mut normalized = DamageRegion::none();

        for &rect in damage.rects() {
            let Some(clipped) = rect.intersection(self.bounds) else {
                continue;
            };

            let aligned = self.align_rect_outward(clipped);
            let Some(clipped_again) = aligned.intersection(self.bounds) else {
                continue;
            };

            // alignment can cause previously separate logical regions to touch or overlap.
            // `DamageRegion::add_rect()` merges those conservatively and keeps storage
            // bounded
            normalized = normalized.add_rect(clipped_again);
        }

        normalized
    }

    fn align_rect_outward(self, rect: Rect) -> Rect {
        let left = align_down_from(rect.x(), self.bounds.x(), self.horizontal_alignment);
        let top = align_down_from(rect.y(), self.bounds.y(), self.vertical_alignment);
        let right = align_up_from(rect.right(), self.bounds.x(), self.horizontal_alignment);
        let bottom = align_up_from(rect.bottom(), self.bounds.y(), self.vertical_alignment);

        Rect::new(Point::new(left, top), Size::new(right - left, bottom - top))
    }
}

fn align_down_from(value: Pixels, origin: Pixels, alignment: Pixels) -> Pixels {
    debug_assert!(alignment.is_positive(),);

    let offset = (value - origin).non_negative().get();
    let alignment = alignment.get();
    let aligned = offset / alignment * alignment;

    origin + px(aligned)
}

fn align_up_from(value: Pixels, origin: Pixels, alignment: Pixels) -> Pixels {
    debug_assert!(alignment.is_positive(),);

    let offset = i64::from((value - origin).non_negative().get());
    let alignment = i64::from(alignment.get());
    let aligned = offset.saturating_add(alignment - 1) / alignment * alignment;
    let aligned = i32::try_from(aligned).unwrap_or(i32::MAX);

    origin + px(aligned)
}

#[cfg(test)]
mod tests {
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
}
