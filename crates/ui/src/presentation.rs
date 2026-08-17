use crate::{DamageRegion, Rect};

pub trait DisplayPresenter {
    type Error;

    fn present_full(&mut self) -> Result<(), Self::Error>;

    fn present_partial(&mut self, regions: &[Rect]) -> Result<(), Self::Error>;

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
}

#[cfg(test)]
mod tests {
    use core::convert::Infallible;

    use crate::{DamageRegion, DisplayPresenter, Point, Rect, Size, px};

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
}
