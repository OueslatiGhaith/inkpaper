use crate::{
    Offset,
    element::state::{ElementStateId, IdentityError},
    storage::{DefaultVec, VecStorage},
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
    slots: DefaultVec<ScrollSlot, SLOTS>,
}

impl<const SLOTS: usize> Default for ScrollStateTable<SLOTS> {
    fn default() -> Self {
        Self {
            slots: DefaultVec::default(),
        }
    }
}

impl<const SLOTS: usize> ScrollStateTable<SLOTS> {
    /// prepare every identity slot before publishing the frame. Input and layout
    /// can then update offsets without allocating.
    pub(crate) fn prepare(&mut self, slots: usize) -> Result<(), IdentityError> {
        if slots > self.slots.len() {
            self.slots
                .try_resize(slots, ScrollSlot::EMPTY)
                .map_err(|error| {
                    error.or(IdentityError::StatesFull, IdentityError::AllocationFailed)
                })?;
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

    pub(crate) fn is_initialized(&self, id: ElementStateId) -> bool {
        self.slots
            .get(id.slot())
            .is_some_and(|slot| slot.initialized && slot.generation == id.generation())
    }

    pub(crate) fn set_offset(&mut self, id: ElementStateId, offset: Offset) {
        let slot = &mut self.slots[id.slot()];
        slot.initialized = true;
        slot.generation = id.generation();
        slot.offset = offset;
    }
}
