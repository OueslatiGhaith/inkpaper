use crate::{ElementId, EntityId, NodeId};

#[cfg(all(test, feature = "alloc"))]
mod alloc_tests;
#[cfg(test)]
mod tests;

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
    pub(crate) created_frame: u32,
    pub(crate) previous_seen_frame: u32,
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
pub enum IdentityError {
    StatesFull,
    DuplicateElementId {
        id: ElementId,
    },
    MissingEntityScope {
        node: NodeId,
    },
    #[cfg(feature = "alloc")]
    AllocationFailed,
}

pub(crate) struct ElementStateTable<const SLOTS: usize> {
    #[cfg(not(feature = "alloc"))]
    slots: [ElementStateSlot; SLOTS],
    #[cfg(feature = "alloc")]
    slots: alloc::vec::Vec<ElementStateSlot>,
}

impl<const SLOTS: usize> Default for ElementStateTable<SLOTS> {
    fn default() -> Self {
        Self {
            #[cfg(not(feature = "alloc"))]
            slots: [ElementStateSlot::vacant(); SLOTS],
            #[cfg(feature = "alloc")]
            slots: alloc::vec::Vec::new(),
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

            entry.previous_seen_frame = entry.last_seen_frame;
            entry.last_seen_frame = frame;

            return Ok(ElementStateId::new(index as u16, slot.generation));
        }

        let index = match self.slots.iter().position(|slot| slot.entry.is_none()) {
            Some(index) => index,
            None => {
                #[cfg(not(feature = "alloc"))]
                return Err(IdentityError::StatesFull);

                #[cfg(feature = "alloc")]
                {
                    let index = self.slots.len();
                    u16::try_from(index).map_err(|_| IdentityError::StatesFull)?;

                    if index == self.slots.capacity() {
                        let additional = if index == 0 { SLOTS.max(1) } else { 1 };
                        self.slots
                            .try_reserve(additional)
                            .map_err(|_| IdentityError::AllocationFailed)?;
                    }

                    self.slots.push(ElementStateSlot::vacant());
                    index
                }
            }
        };

        let index = u16::try_from(index).map_err(|_| IdentityError::StatesFull)?;
        let slot = &mut self.slots[usize::from(index)];

        slot.entry = Some(ElementStateEntry {
            key,
            created_frame: frame,
            previous_seen_frame: frame,
            last_seen_frame: frame,
        });

        Ok(ElementStateId::new(index, slot.generation))
    }

    /// includes vacant slots whose generations must survive reuse.
    pub(crate) fn slot_count(&self) -> usize {
        self.slots.len()
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

    pub(crate) fn abort_frame(&mut self, frame: u32) {
        for slot in &mut self.slots {
            let created_this_frame =
                matches!(slot.entry, Some(entry) if entry.created_frame == frame);

            if created_this_frame {
                slot.entry = None;
                slot.generation = slot.generation.wrapping_add(1);
                continue;
            }

            let Some(entry) = slot.entry.as_mut() else {
                continue;
            };
            if entry.last_seen_frame == frame {
                entry.last_seen_frame = entry.previous_seen_frame;
            }
        }
    }

    pub(crate) fn clear(&mut self) {
        for slot in &mut self.slots {
            if slot.entry.is_some() {
                slot.entry = None;
                slot.generation = slot.generation.wrapping_add(1);
            }
        }
    }
}
