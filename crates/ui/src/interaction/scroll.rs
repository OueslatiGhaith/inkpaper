use crate::{
    Offset,
    element::state::{ElementStateId, IdentityError},
};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScrollAxes {
    #[default]
    None,
    Horizontal,
    Vertical,
    Both,
}

impl ScrollAxes {
    pub(crate) const fn horizontal(self) -> bool {
        matches!(self, Self::Horizontal | Self::Both)
    }

    pub(crate) const fn vertical(self) -> bool {
        matches!(self, Self::Vertical | Self::Both)
    }

    pub(crate) const fn any(self) -> bool {
        !matches!(self, Self::None)
    }
}

#[derive(Debug, Clone, Copy)]
struct ScrollSlot {
    initialized: bool,
    generation: u32,
    offset: Offset,
}

impl ScrollSlot {
    const EMPTY: Self = Self {
        initialized: false,
        generation: 0,
        offset: Offset::ZERO,
    };
}

pub(crate) struct ScrollStateTable<const SLOTS: usize> {
    #[cfg(not(feature = "alloc"))]
    slots: [ScrollSlot; SLOTS],
    #[cfg(feature = "alloc")]
    slots: alloc::vec::Vec<ScrollSlot>,
}

impl<const SLOTS: usize> Default for ScrollStateTable<SLOTS> {
    fn default() -> Self {
        Self {
            #[cfg(not(feature = "alloc"))]
            slots: [ScrollSlot::EMPTY; SLOTS],
            #[cfg(feature = "alloc")]
            slots: alloc::vec::Vec::new(),
        }
    }
}

impl<const SLOTS: usize> ScrollStateTable<SLOTS> {
    /// prepare every identity slot before publishing the frame. Input and layout
    /// can then update offsets without allocating.
    pub(crate) fn prepare(&mut self, slots: usize) -> Result<(), IdentityError> {
        #[cfg(not(feature = "alloc"))]
        if slots > SLOTS {
            return Err(IdentityError::StatesFull);
        }

        #[cfg(feature = "alloc")]
        if slots > self.slots.len() {
            let target = if self.slots.capacity() == 0 {
                slots.max(SLOTS)
            } else {
                slots
            };

            self.slots
                .try_reserve(target - self.slots.len())
                .map_err(|_| IdentityError::AllocationFailed)?;
            self.slots.resize(slots, ScrollSlot::EMPTY);
        }

        Ok(())
    }

    pub(crate) fn offset(&self, id: ElementStateId) -> Offset {
        let Some(slot) = self.slots.get(id.slot()) else {
            return Offset::ZERO;
        };

        if slot.initialized && slot.generation == id.generation() {
            slot.offset
        } else {
            Offset::ZERO
        }
    }

    pub(crate) fn set_offset(&mut self, id: ElementStateId, offset: Offset) {
        let slot = &mut self.slots[id.slot()];
        slot.initialized = true;
        slot.generation = id.generation();
        slot.offset = offset;
    }
}
