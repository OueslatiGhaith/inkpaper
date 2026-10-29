use core::fmt;

use crate::{Point, Rect, Size};

const EMPTY_RECT: Rect = Rect::new(Point::ZERO, Size::ZERO);

/// up to `N` rectangles, kept in insertion order and copied by value. Only the stored
/// rectangles are reachable, so free slots can't leak into iteration or comparison
#[derive(Clone, Copy)]
pub(super) struct RectList<const N: usize> {
    rects: [Rect; N],
    len: u8,
}

impl<const N: usize> RectList<N> {
    const LEN_FITS_U8: () = assert!(N <= u8::MAX as usize, "capacity must fit in a u8");

    pub(super) const fn new() -> Self {
        #[allow(clippy::let_unit_value)]
        let () = Self::LEN_FITS_U8;

        Self {
            rects: [EMPTY_RECT; N],
            len: 0,
        }
    }

    pub(super) const fn len(&self) -> usize {
        self.len as usize
    }

    pub(super) fn as_slice(&self) -> &[Rect] {
        &self.rects[..self.len()]
    }

    pub(super) fn as_mut_slice(&mut self) -> &mut [Rect] {
        let len = self.len();
        &mut self.rects[..len]
    }

    /// gives the rectangle back when the list is full
    pub(super) fn push(&mut self, rect: Rect) -> Result<(), Rect> {
        let len = self.len();
        if len == N {
            return Err(rect);
        }

        self.rects[len] = rect;
        self.len += 1;

        Ok(())
    }

    /// removes the rectangle at `index`, keeping the order of the others
    pub(super) fn remove(&mut self, index: usize) -> Rect {
        let removed = self.as_slice()[index];
        self.as_mut_slice()[index..].rotate_left(1);
        self.len -= 1;

        removed
    }

    pub(super) fn clear(&mut self) {
        self.len = 0;
    }
}

impl<const N: usize> PartialEq for RectList<N> {
    fn eq(&self, other: &Self) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl<const N: usize> Eq for RectList<N> {}

impl<const N: usize> fmt::Debug for RectList<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.as_slice()).finish()
    }
}

#[cfg(feature = "defmt")]
impl<const N: usize> defmt::Format for RectList<N> {
    fn format(&self, f: defmt::Formatter<'_>) {
        defmt::write!(f, "{}", self.as_slice())
    }
}
