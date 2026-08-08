use heapless::index_map::Entry;

use crate::{ElementId, EntityId, NodeId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ElementStateId {
    slot: u16,
    generation: u32,
}

impl ElementStateId {
    const fn new(slot: u16, generation: u32) -> Self {
        Self { slot, generation }
    }

    pub(crate) const fn slot(self) -> usize {
        self.slot as usize
    }

    pub(crate) const fn generation(self) -> u32 {
        self.generation
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IdentityParent {
    Entity(EntityId),
    Element(ElementStateId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ElementStateKey {
    pub(crate) parent: IdentityParent,
    pub(crate) local: ElementId,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ElementStateEntry {
    pub(crate) key: ElementStateKey,
    pub(crate) last_seen_frame: u32,
}

#[derive(Debug, Clone, Copy)]
struct ElementStateSlot {
    generation: u32,
    entry: Option<ElementStateEntry>,
}

impl ElementStateSlot {
    const fn vacant() -> Self {
        Self {
            generation: 0,
            entry: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IdentityError {
    StatesFull,
    DuplicateElementId { id: ElementId },
    MissingEntityScope { node: NodeId },
}

pub(crate) struct ElementStateTable<const SLOTS: usize> {
    slots: [ElementStateSlot; SLOTS],
}

impl<const SLOTS: usize> Default for ElementStateTable<SLOTS> {
    fn default() -> Self {
        Self {
            slots: [ElementStateSlot::vacant(); SLOTS],
        }
    }
}

impl<const SLOTS: usize> ElementStateTable<SLOTS> {
    pub(crate) fn resolve(
        &mut self,
        parent: IdentityParent,
        local: ElementId,
        frame: u32,
    ) -> Result<ElementStateId, IdentityError> {
        let key = ElementStateKey { parent, local };

        // first look for an existing identity
        for (index, slot) in self.slots.iter_mut().enumerate() {
            let Some(entry) = slot.entry.as_mut() else {
                continue;
            };
            if entry.key != key {
                continue;
            }

            // seeing the exact same key twice during one frame means duplicate Ids
            // within the same identity scope
            if entry.last_seen_frame == frame {
                return Err(IdentityError::DuplicateElementId { id: local });
            }

            entry.last_seen_frame = frame;

            return Ok(ElementStateId::new(index as u16, slot.generation));
        }

        // new logical element: find a vacant slot
        for (index, slot) in self.slots.iter_mut().enumerate() {
            if slot.entry.is_some() {
                continue;
            }
            if index > u16::MAX as usize {
                return Err(IdentityError::StatesFull);
            }

            slot.entry = Some(ElementStateEntry {
                key,
                last_seen_frame: frame,
            });

            return Ok(ElementStateId::new(index as u16, slot.generation));
        }

        Err(IdentityError::StatesFull)
    }

    pub(crate) fn sweep(&mut self, current_frame: u32) {
        for slot in &mut self.slots {
            let should_remove = match slot.entry {
                Some(entry) => entry.last_seen_frame != current_frame,
                None => false,
            };

            if should_remove {
                slot.entry = None;
                slot.generation = slot.generation.wrapping_add(1);
            }
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.slots
            .iter()
            .filter(|slot| slot.entry.is_some())
            .count()
    }

    pub(crate) fn contains(&self, id: ElementStateId) -> bool {
        let Some(slot) = self.slots.get(id.slot()) else {
            return false;
        };

        slot.generation == id.generation() && slot.entry.is_some()
    }

    pub(crate) fn entry(&self, id: ElementStateId) -> Option<&ElementStateEntry> {
        let slot = self.slots.get(id.slot())?;
        if slot.generation != id.generation {
            return None;
        }

        slot.entry.as_ref()
    }
}
