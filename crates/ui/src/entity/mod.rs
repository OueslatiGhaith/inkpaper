mod arena;
mod store;

#[cfg(feature = "alloc")]
pub use arena::HeapEntityArena;
pub use arena::{EntityAccessError, EntityAllocError, FixedEntityArena};

// unit tests run against the storage the `alloc` feature implies
#[cfg(all(test, not(feature = "alloc")))]
pub(crate) type TestEntityArena<const BYTES: usize, const SLOTS: usize> =
    FixedEntityArena<BYTES, SLOTS>;
#[cfg(all(test, feature = "alloc"))]
pub(crate) type TestEntityArena<const BYTES: usize, const SLOTS: usize> = HeapEntityArena<SLOTS>;

#[cfg(test)]
pub(crate) use store::drop_value;
pub(crate) use store::{EntityStore, RawEntityBorrow, RawEntityReservation, create_entity};

use core::{any::TypeId, marker::PhantomData};

use crate::{
    BorrowKind, Context, Element, MountCx, MountError, NodeId, Render, render_entity,
    runtime::Source,
};

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
    /// reads the entity's state, and records the read so the entity being rendered, if
    /// any, renders again when this one changes.
    ///
    /// # Panics
    ///
    /// when the entity is gone, is still being built, or is being updated. See
    /// [`Self::try_read`]
    #[track_caller]
    pub fn read<C, R>(self, cx: &Context<'_, C>, f: impl FnOnce(&T) -> R) -> R
    where
        C: 'static,
    {
        match self.try_read(cx, f) {
            Ok(value) => value,
            Err(error) => panic!("could not read entity: {error:?}"),
        }
    }

    /// updates the entity's state through its own [`Context`].
    ///
    /// # Panics
    ///
    /// when the entity is gone, is still being built, or is already borrowed. See
    /// [`Self::try_update`]
    #[track_caller]
    pub fn update<C, R>(
        self,
        cx: &Context<'_, C>,
        f: impl FnOnce(&mut T, &mut Context<'_, T>) -> R,
    ) -> R
    where
        C: 'static,
    {
        match self.try_update(cx, f) {
            Ok(value) => value,
            Err(error) => panic!("could not update entity: {error:?}"),
        }
    }

    /// marks the entity as changed, so it and the entities that read it render again.
    /// Unlike [`Context::notify`] through [`Self::update`], it doesn't borrow the entity
    pub fn notify<C>(self, cx: &Context<'_, C>) {
        cx.runtime.notify(self.id);
    }

    pub fn try_read<C, R>(
        self,
        cx: &Context<'_, C>,
        f: impl FnOnce(&T) -> R,
    ) -> Result<R, EntityAccessError>
    where
        C: 'static,
    {
        cx.runtime.record_read(Source::Entity(self.id));

        let borrow = RawEntityBorrow::acquire(
            cx.runtime.entities,
            self.id,
            TypeId::of::<T>(),
            BorrowKind::Shared,
        )?;

        let value = unsafe { &*borrow.ptr().cast::<T>().as_ptr() };

        Ok(f(value))
    }

    pub fn try_update<C, R>(
        self,
        cx: &Context<'_, C>,
        f: impl FnOnce(&mut T, &mut Context<'_, T>) -> R,
    ) -> Result<R, EntityAccessError>
    where
        C: 'static,
    {
        cx.runtime.record_read(Source::Entity(self.id));

        let borrow = RawEntityBorrow::acquire(
            cx.runtime.entities,
            self.id,
            TypeId::of::<T>(),
            BorrowKind::Exclusive,
        )?;

        let value = unsafe { &mut *borrow.ptr().cast::<T>().as_ptr() };
        let mut entity_cx = Context::new_in(self, cx.runtime);

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
