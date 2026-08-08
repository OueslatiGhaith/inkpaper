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

            entry.previous_seen_frame = entry.last_seen_frame;
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
                created_frame: frame,
                previous_seen_frame: frame,
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

#[cfg(test)]
mod tests {
    use crate::{
        element_state::{ElementStateTable, IdentityError, IdentityParent},
        *,
    };

    fn entity_parent(slot: u16) -> IdentityParent {
        IdentityParent::Entity(EntityId::new(slot, 0))
    }

    #[test]
    fn resolves_new_element_identity() {
        let mut states = ElementStateTable::<8>::default();

        let id = states
            .resolve(entity_parent(0), ElementId::Name("button"), 1)
            .unwrap();

        assert!(states.contains(id));
        assert_eq!(states.len(), 1);
    }

    #[test]
    fn identity_is_stable_across_frames() {
        let mut states = ElementStateTable::<8>::default();

        let parent = entity_parent(0);

        let first = states
            .resolve(parent, ElementId::Name("button"), 1)
            .unwrap();
        states.sweep(1);

        let second = states
            .resolve(parent, ElementId::Name("button"), 2)
            .unwrap();
        states.sweep(2);

        assert_eq!(first, second,);
        assert!(states.contains(first));
    }

    #[test]
    fn duplicate_identity_in_same_frame_is_rejected() {
        let mut states = ElementStateTable::<8>::default();

        let parent = entity_parent(0);

        states
            .resolve(parent, ElementId::Name("button"), 1)
            .unwrap();

        let result = states.resolve(parent, ElementId::Name("button"), 1);

        assert_eq!(
            result,
            Err(IdentityError::DuplicateElementId {
                id: ElementId::Name("button"),
            }),
        );
    }

    #[test]
    fn same_local_id_is_allowed_under_different_entities() {
        let mut states = ElementStateTable::<8>::default();

        let first = states
            .resolve(entity_parent(0), ElementId::Name("button"), 1)
            .unwrap();

        let second = states
            .resolve(entity_parent(1), ElementId::Name("button"), 1)
            .unwrap();

        assert_ne!(first, second,);
        assert_eq!(states.len(), 2,);
    }

    #[test]
    fn same_local_id_is_allowed_under_different_element_parents() {
        let mut states = ElementStateTable::<8>::default();

        let root = entity_parent(0);
        let left = states.resolve(root, ElementId::Name("left"), 1).unwrap();
        let right = states.resolve(root, ElementId::Name("right"), 1).unwrap();
        let left_button = states
            .resolve(IdentityParent::Element(left), ElementId::Name("button"), 1)
            .unwrap();
        let right_button = states
            .resolve(IdentityParent::Element(right), ElementId::Name("button"), 1)
            .unwrap();

        assert_ne!(left_button, right_button,);
        assert_eq!(states.len(), 4,);
    }

    #[test]
    fn nested_identity_uses_parent_element_state() {
        let mut states = ElementStateTable::<8>::default();

        let root = entity_parent(0);
        let panel = states.resolve(root, ElementId::Name("panel"), 1).unwrap();
        let button = states
            .resolve(IdentityParent::Element(panel), ElementId::Name("button"), 1)
            .unwrap();

        assert_ne!(panel, button,);

        let button_slot = &states.slots[button.slot()];
        let button_entry = button_slot.entry.as_ref().unwrap();

        assert_eq!(button_entry.key.parent, IdentityParent::Element(panel),);
        assert_eq!(button_entry.key.local, ElementId::Name("button"),);
    }

    #[test]
    fn sweep_removes_elements_not_seen_in_current_frame() {
        let mut states = ElementStateTable::<8>::default();

        let parent = entity_parent(0);
        let button = states
            .resolve(parent, ElementId::Name("button"), 1)
            .unwrap();
        states.sweep(1);

        assert!(states.contains(button));

        // frame 2 doesn't resolve "button".
        states.sweep(2);

        assert!(!states.contains(button));
        assert_eq!(states.len(), 0,);
    }

    #[test]
    fn reappearing_element_gets_new_generation() {
        let mut states = ElementStateTable::<8>::default();

        let parent = entity_parent(0);

        let old = states
            .resolve(parent, ElementId::Name("button"), 1)
            .unwrap();
        states.sweep(1);

        // button disappears during frame 2.
        states.sweep(2);

        assert!(!states.contains(old));

        let new = states
            .resolve(parent, ElementId::Name("button"), 3)
            .unwrap();
        states.sweep(3);

        assert_ne!(old, new,);

        // with the current first-free-slot strategy, the slot should normally be
        // reused, but with a new generation.
        assert_eq!(old.slot(), new.slot());
        assert_ne!(old.generation(), new.generation());
    }

    #[test]
    fn existing_entry_tracks_previous_seen_frame() {
        let mut states = ElementStateTable::<8>::default();

        let parent = entity_parent(0);
        let id = states
            .resolve(parent, ElementId::Name("button"), 1)
            .unwrap();
        states.sweep(1);

        states
            .resolve(parent, ElementId::Name("button"), 2)
            .unwrap();

        let entry = states.slots[id.slot()].entry.as_ref().unwrap();

        assert_eq!(entry.created_frame, 1,);
        assert_eq!(entry.previous_seen_frame, 1,);
        assert_eq!(entry.last_seen_frame, 2,);
    }

    #[test]
    fn newly_created_entry_records_creation_frame() {
        let mut states = ElementStateTable::<8>::default();

        let id = states
            .resolve(entity_parent(0), ElementId::Name("button"), 42)
            .unwrap();

        let entry = states.slots[id.slot()].entry.as_ref().unwrap();

        assert_eq!(entry.created_frame, 42);
        assert_eq!(entry.previous_seen_frame, 42);
        assert_eq!(entry.last_seen_frame, 42);
    }

    #[test]
    fn abort_removes_elements_created_during_failed_frame() {
        let mut states = ElementStateTable::<8>::default();

        let parent = entity_parent(0);
        let existing = states
            .resolve(parent, ElementId::Name("existing"), 1)
            .unwrap();
        states.sweep(1);

        let created = states
            .resolve(parent, ElementId::Name("created"), 2)
            .unwrap();

        assert!(states.contains(created));
        assert_eq!(states.len(), 2,);

        states.abort_frame(2);

        assert!(states.contains(existing));
        assert!(!states.contains(created));
        assert_eq!(states.len(), 1,);
    }

    #[test]
    fn abort_restores_existing_entry_last_seen_frame() {
        let mut states = ElementStateTable::<8>::default();

        let parent = entity_parent(0);
        let id = states
            .resolve(parent, ElementId::Name("button"), 1)
            .unwrap();
        states.sweep(1);

        states
            .resolve(parent, ElementId::Name("button"), 2)
            .unwrap();

        {
            let entry = states.slots[id.slot()].entry.as_ref().unwrap();

            assert_eq!(entry.previous_seen_frame, 1,);
            assert_eq!(entry.last_seen_frame, 2,);
        }

        states.abort_frame(2);

        let entry = states.slots[id.slot()].entry.as_ref().unwrap();

        assert_eq!(entry.last_seen_frame, 1);
        assert!(states.contains(id));
    }

    #[test]
    fn aborted_existing_identity_remains_stable_next_frame() {
        let mut states = ElementStateTable::<8>::default();

        let parent = entity_parent(0);

        let original = states
            .resolve(parent, ElementId::Name("button"), 1)
            .unwrap();
        states.sweep(1);

        // failed frame.
        let during_failed_frame = states
            .resolve(parent, ElementId::Name("button"), 2)
            .unwrap();

        assert_eq!(original, during_failed_frame,);

        states.abort_frame(2);

        // next valid frame.
        let after_abort = states
            .resolve(parent, ElementId::Name("button"), 3)
            .unwrap();
        states.sweep(3);

        assert_eq!(original, after_abort,);
    }

    #[test]
    fn failed_frame_can_remove_new_and_restore_existing_entries_together() {
        let mut states = ElementStateTable::<8>::default();

        let parent = entity_parent(0);
        let old = states.resolve(parent, ElementId::Name("old"), 1).unwrap();
        states.sweep(1);

        // frame 2 starts successfully touching existing state...
        let same_old = states.resolve(parent, ElementId::Name("old"), 2).unwrap();

        assert_eq!(old, same_old,);

        // ...and creates new state.
        let new = states.resolve(parent, ElementId::Name("new"), 2).unwrap();

        assert_eq!(states.len(), 2,);

        // something later in frame building fails.
        states.abort_frame(2);

        assert!(states.contains(old));
        assert!(!states.contains(new));
        assert_eq!(states.len(), 1,);

        let old_entry = states.slots[old.slot()].entry.as_ref().unwrap();

        assert_eq!(old_entry.last_seen_frame, 1,);
    }

    #[test]
    fn clearing_table_invalidates_existing_ids() {
        let mut states = ElementStateTable::<8>::default();

        let id = states
            .resolve(entity_parent(0), ElementId::Name("button"), 1)
            .unwrap();
        states.sweep(1);

        assert!(states.contains(id));

        states.clear();

        assert!(!states.contains(id));
        assert_eq!(states.len(), 0,);
    }

    #[test]
    fn clear_changes_generation_when_slot_is_reused() {
        let mut states = ElementStateTable::<8>::default();

        let parent = entity_parent(0);
        let old = states
            .resolve(parent, ElementId::Name("button"), 1)
            .unwrap();
        states.clear();

        let new = states
            .resolve(parent, ElementId::Name("button"), 2)
            .unwrap();

        assert_eq!(old.slot(), new.slot(),);
        assert_ne!(old.generation(), new.generation(),);
        assert_ne!(old, new,);
    }

    #[test]
    fn reports_state_capacity_exhaustion() {
        let mut states = ElementStateTable::<1>::default();

        let parent = entity_parent(0);
        states.resolve(parent, ElementId::Name("first"), 1).unwrap();

        let result = states.resolve(parent, ElementId::Name("second"), 1);

        assert_eq!(result, Err(IdentityError::StatesFull),);
    }
}
