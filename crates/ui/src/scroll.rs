use crate::{Offset, element_state::ElementStateId};

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
    slots: [ScrollSlot; SLOTS],
}

impl<const SLOTS: usize> Default for ScrollStateTable<SLOTS> {
    fn default() -> Self {
        Self {
            slots: [ScrollSlot::EMPTY; SLOTS],
        }
    }
}

impl<const SLOTS: usize> ScrollStateTable<SLOTS> {
    pub(crate) fn offset(&self, id: ElementStateId) -> Offset {
        let slot = &self.slots[id.slot()];
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
