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
mod tests;
