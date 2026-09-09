#[cfg(feature = "alloc")]
mod allocated;
mod arena;
mod store;

pub use arena::{EntityAccessError, EntityAllocError};

#[cfg(feature = "alloc")]
pub(crate) use allocated::EntityArena;
#[cfg(not(feature = "alloc"))]
pub(crate) use arena::EntityArena;
pub(crate) use arena::{EntityBorrowKind, align_up};

pub(crate) use store::{
    BorrowState, EntityStore, RawEntityBorrow, RawEntityReservation, create_entity, drop_value,
};

use core::{any::TypeId, marker::PhantomData};

use crate::{Context, Element, MountCx, MountError, NodeId, Render, render_entity};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct EntityId {
    slot: u16,
    generation: u16,
}

impl EntityId {
    pub(crate) const fn new(slot: u16, generation: u16) -> Self {
        Self { slot, generation }
    }

    pub(crate) const fn slot(self) -> u16 {
        self.slot
    }

    pub(crate) const fn generation(self) -> u16 {
        self.generation
    }
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Entity<T> {
    id: EntityId,
    _marker: PhantomData<fn() -> T>,
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

impl<T: 'static> Entity<T> {
    pub fn read<C, R>(
        self,
        cx: &Context<'_, C>,
        f: impl FnOnce(&T) -> R,
    ) -> Result<R, EntityAccessError>
    where
        C: 'static,
    {
        let borrow = RawEntityBorrow::acquire(
            cx.store,
            self.id,
            TypeId::of::<T>(),
            EntityBorrowKind::Shared,
        )?;

        let value = unsafe { &*borrow.ptr().cast::<T>().as_ptr() };

        Ok(f(value))
    }

    pub fn update<C, R>(
        self,
        cx: &Context<'_, C>,
        f: impl FnOnce(&mut T, &mut Context<'_, T>) -> R,
    ) -> Result<R, EntityAccessError>
    where
        C: 'static,
    {
        let borrow = RawEntityBorrow::acquire(
            cx.store,
            self.id,
            TypeId::of::<T>(),
            EntityBorrowKind::Exclusive,
        )?;

        let value = unsafe { &mut *borrow.ptr().cast::<T>().as_ptr() };
        let mut entity_cx =
            Context::from_parts(self, cx.store, cx.globals, cx.callbacks, cx.notified);

        Ok(f(value, &mut entity_cx))
    }
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

impl<T> Element for Entity<T>
where
    T: Render,
{
    fn mount(self, cx: &mut MountCx<'_>) -> Result<NodeId, MountError> {
        cx.push_entity(self.entity_id(), render_entity::<T>)
    }
}
