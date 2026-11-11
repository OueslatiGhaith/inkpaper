mod arena;
mod lease;
mod release;
mod store;

#[cfg(test)]
mod read_tests;
#[cfg(test)]
mod release_tests;

pub(crate) use arena::EntityArena;
pub use arena::{EntityAccessError, EntityAllocError};
pub(crate) use lease::{LeaseOwner, LeaseStore, LeaseTable, NoLeases};
pub use release::ReleaseError;
pub(crate) use release::{NoReleases, ReleaseQueue, ReleaseStore};

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
    /// borrows the entity's state for as long as `cx` lives, and records the read so
    /// the entity being rendered, if any, renders again when this one changes.
    ///
    /// the entity stays borrowed until `cx` drops, so updating it meanwhile fails. A
    /// render's context lives until its element is mounted, so the element can borrow
    /// from what the render read.
    ///
    /// # Panics
    ///
    /// when the entity is gone, is still being built, or is being updated. See
    /// [`Self::try_read`]
    #[track_caller]
    pub fn read<'c, C>(self, cx: &'c Context<'_, C>) -> &'c T
    where
        C: 'static,
    {
        match self.try_read(cx) {
            Ok(value) => value,
            Err(error) => panic!("could not read entity: {error:?}"),
        }
    }

    pub fn try_read<'c, C>(self, cx: &'c Context<'_, C>) -> Result<&'c T, EntityAccessError>
    where
        C: 'static,
    {
        cx.runtime.record_read(Source::Entity(self.id));

        let entities = cx.runtime.entities;
        let ptr = entities.borrow(self.id, TypeId::of::<T>(), BorrowKind::Shared)?;

        // one lease per context and entity keeps the entity borrowed. A second read
        // through the same context gives its borrow back at once
        if cx.runtime.leases.holds(cx.lease_owner, self.id) {
            entities.release(self.id, BorrowKind::Shared);
        } else if let Err(error) = cx.runtime.leases.push(cx.lease_owner, self.id) {
            entities.release(self.id, BorrowKind::Shared);

            return Err(EntityAccessError::OutOfMemory(error));
        }

        // SAFETY: the store checked the type, and the lease keeps the shared borrow
        // until `cx` drops, which can't happen while the reference lives
        Ok(unsafe { &*ptr.cast::<T>().as_ptr() })
    }

    /// reads the entity's state for the length of `f`, and records the read like
    /// [`Self::read`]. Unlike it, the entity is borrowed only while `f` runs.
    ///
    /// # Panics
    ///
    /// when the entity is gone, is still being built, or is being updated. See
    /// [`Self::try_read_with`]
    #[track_caller]
    pub fn read_with<C, R>(self, cx: &Context<'_, C>, f: impl FnOnce(&T) -> R) -> R
    where
        C: 'static,
    {
        match self.try_read_with(cx, f) {
            Ok(value) => value,
            Err(error) => panic!("could not read entity: {error:?}"),
        }
    }

    pub fn try_read_with<C, R>(
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
