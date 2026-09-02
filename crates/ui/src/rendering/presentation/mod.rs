use crate::{DamageRegion, Pixels, Point, Rect, Size, px};

#[cfg(test)]
mod tests;

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
