use core::marker::PhantomData;

use crate::{Element, Render};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EntityId {
    slot: u16,
    generation: u16,
}

impl EntityId {
    pub(crate) const fn new(slot: u16, generation: u16) -> Self {
        Self { slot, generation }
    }
}

pub struct Entity<T> {
    id: EntityId,
    _marker: PhantomData<fn() -> T>,
}

impl<T> Copy for Entity<T> {}

impl<T> Clone for Entity<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> PartialEq for Entity<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T> Eq for Entity<T> {}

impl<T> core::fmt::Debug for Entity<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("Entity").field(&self.id).finish()
    }
}

impl<T> Entity<T> {
    pub(crate) const fn from_id(id: EntityId) -> Self {
        Self {
            id,
            _marker: PhantomData,
        }
    }

    pub const fn entity_id(self) -> EntityId {
        self.id
    }
}

impl<T> Element for Entity<T> where T: Render {}
