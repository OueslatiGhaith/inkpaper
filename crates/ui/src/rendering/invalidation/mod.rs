use crate::{Offset, Point, Rect, Size};

use rects::RectList;

mod rects;
#[cfg(test)]
mod tests;

const DAMAGE_RECT_CAPACITY: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct DamageRegion {
    rects: RectList<DAMAGE_RECT_CAPACITY>,
    full: bool,
}

impl Default for DamageRegion {
    fn default() -> Self {
        Self::none()
    }
}

impl DamageRegion {
    pub const fn none() -> Self {
        Self {
            rects: RectList::new(),
            full: false,
        }
    }

    pub const fn full() -> Self {
        Self {
            rects: RectList::new(),
            full: true,
        }
    }

    pub fn from_rect(rect: Rect) -> Self {
        Self::none().add_rect(rect)
    }

    pub const fn is_none(self) -> bool {
        !self.full && self.rects.len() == 0
    }

    pub const fn is_full(self) -> bool {
        self.full
    }

    // a full region has no rects, so `is_none` is the emptiness check
    #[allow(clippy::len_without_is_empty)]
    pub const fn len(self) -> usize {
        self.rects.len()
    }

    pub fn rects(&self) -> &[Rect] {
        self.rects.as_slice()
    }

    pub fn add_rect(mut self, rect: Rect) -> Self {
        if self.full || rect.width().is_non_positive() || rect.height().is_non_positive() {
            return self;
        }

        let mut candidate = rect;
        let mut index = 0usize;

        while index < self.rects.len() {
            let existing = self.rects.as_slice()[index];

            if damage_rects_touch_or_overlap(candidate, existing) {
                candidate = damage_rect_union(candidate, existing);

                self.rects.remove(index);

                /*
                 * The enlarged candidate can now overlap
                 * a rectangle we checked earlier, so
                 * restart the small bounded scan.
                 */
                index = 0;
            } else {
                index += 1;
            }
        }

        let Err(candidate) = self.rects.push(candidate) else {
            return self;
        };

        // damage storage is deliberately bounded. If it fills up, correctness wins over
        // precision: collapse all rectangles into one conservative bounding rectangle.
        let collapsed = self
            .rects
            .as_slice()
            .iter()
            .copied()
            .fold(candidate, damage_rect_union);

        self.rects.clear();
        self.rects
            .push(collapsed)
            .expect("an emptied damage list has room");

        self
    }

    pub fn merge(mut self, other: Self) -> Self {
        if self.full || other.full {
            return Self::full();
        }

        for &rect in other.rects() {
            self = self.add_rect(rect);
        }

        self
    }

    pub fn translated(mut self, delta: Offset) -> Self {
        if self.full {
            return self;
        }

        for rect in self.rects.as_mut_slice() {
            *rect = rect.translated(delta);
        }

        self
    }

    pub fn clipped_to(self, clip: Rect) -> Self {
        if clip.width().is_non_positive() || clip.height().is_non_positive() {
            return Self::none();
        }
        if self.full {
            return Self::from_rect(clip);
        }

        let mut clipped = Self::none();

        for rect in self.rects() {
            if let Some(intersection) = rect.intersection(clip) {
                clipped = clipped.add_rect(intersection);
            }
        }

        clipped
    }

    pub fn intersects_rect(self, rect: Rect) -> bool {
        if rect.width().is_non_positive() || rect.height().is_non_positive() {
            return false;
        }
        if self.full {
            return true;
        }

        self.rects()
            .iter()
            .any(|damage| damage.intersection(rect).is_some())
    }

    pub fn partial_damage_bounds(&self) -> Rect {
        debug_assert!(!self.is_none(), "empty damage has no bounds");
        debug_assert!(!self.is_full(), "full damage has no finite boubds");

        let mut rects = self.rects().iter().copied();
        let first = rects
            .next()
            .expect("non-empty partial damage must contain a rectangle");

        rects.fold(first, Rect::union)
    }
}

fn damage_rect_union(first: Rect, second: Rect) -> Rect {
    let left = first.x().min(second.x());
    let top = first.y().min(second.y());
    let right = first.right().max(second.right());
    let bottom = first.bottom().max(second.bottom());

    Rect::new(Point::new(left, top), Size::new(right - left, bottom - top))
}

fn damage_rects_touch_or_overlap(first: Rect, second: Rect) -> bool {
    !(first.right() < second.x()
        || second.right() < first.x()
        || first.bottom() < second.y()
        || second.bottom() < first.y())
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Invalidation {
    #[default]
    None,
    Paint,
    Layout,
    Rebuild,
}

impl Invalidation {
    pub const fn merge(self, other: Self) -> Self {
        use Invalidation::*;

        match (self, other) {
            (Rebuild, _) | (_, Rebuild) => Rebuild,
            (Layout, _) | (_, Layout) => Layout,
            (Paint, _) | (_, Paint) => Paint,
            _ => None,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RenderInvalidation {
    kind: Invalidation,
    damage: DamageRegion,
}

impl RenderInvalidation {
    pub const fn none() -> Self {
        Self {
            kind: Invalidation::None,
            damage: DamageRegion::none(),
        }
    }

    pub const fn full(kind: Invalidation) -> Self {
        match kind {
            Invalidation::None => Self::none(),
            _ => Self {
                kind,
                damage: DamageRegion::full(),
            },
        }
    }

    pub fn damaged(kind: Invalidation, damage: DamageRegion) -> Self {
        match kind {
            Invalidation::None => Self::none(),
            Invalidation::Paint => {
                if damage.is_none() {
                    Self::none()
                } else {
                    Self { kind, damage }
                }
            }
            Invalidation::Layout | Invalidation::Rebuild => Self::full(kind),
        }
    }

    pub const fn kind(self) -> Invalidation {
        self.kind
    }

    pub const fn damage(self) -> DamageRegion {
        self.damage
    }

    pub const fn is_none(self) -> bool {
        matches!(self.kind, Invalidation::None)
    }

    pub fn merge(self, other: Self) -> Self {
        let kind = self.kind.merge(other.kind);
        let damage = self.damage.merge(other.damage);

        Self::damaged(kind, damage)
    }
}
