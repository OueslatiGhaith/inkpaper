use core::{alloc::Layout, any::TypeId, cell::Cell, ptr::NonNull};

use crate::{
    Context, Entity, EntityAccessError, EntityAllocError, EntityBorrowKind, EntityId,
    callback_store::CallbackStore, global::GlobalStore,
};

pub(crate) struct RawEntityReservation {
    pub(crate) id: EntityId,
    pub(crate) ptr: NonNull<u8>,
}

pub(crate) unsafe trait EntityStore {
    fn reserve(
        &self,
        layout: Layout,
        type_id: TypeId,
        drop_fn: unsafe fn(*mut u8),
    ) -> Result<RawEntityReservation, EntityAllocError>;

    /// # Safety
    ///
    /// the reserved storage for this entity must contain a valid initialized value
    /// of the type registered during `reserve`
    unsafe fn commit(&self, entity: EntityId);

    fn abandon(&self, entity: EntityId);

    fn borrow(
        &self,
        entity: EntityId,
        type_id: TypeId,
        kind: EntityBorrowKind,
    ) -> Result<NonNull<u8>, EntityAccessError>;

    fn release(&self, entity: EntityId, kind: EntityBorrowKind);
}

pub(crate) unsafe fn drop_value<T>(ptr: *mut u8) {
    unsafe { core::ptr::drop_in_place(ptr.cast::<T>()) };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BorrowState {
    Free,
    Shared(u16),
    Exclusive,
}

pub(crate) struct RawEntityBorrow<'a> {
    store: &'a dyn EntityStore,
    entity: EntityId,
    ptr: NonNull<u8>,
    kind: EntityBorrowKind,
}

impl<'a> RawEntityBorrow<'a> {
    pub fn acquire(
        store: &'a dyn EntityStore,
        entity: EntityId,
        type_id: TypeId,
        kind: EntityBorrowKind,
    ) -> Result<Self, EntityAccessError> {
        let ptr = store.borrow(entity, type_id, kind)?;

        Ok(Self {
            store,
            entity,
            ptr,
            kind,
        })
    }

    pub fn ptr(&self) -> NonNull<u8> {
        self.ptr
    }
}

impl Drop for RawEntityBorrow<'_> {
    fn drop(&mut self) {
        self.store.release(self.entity, self.kind);
    }
}

struct ReservationGuard<'a> {
    store: &'a dyn EntityStore,
    entity: EntityId,
    armed: bool,
}

impl ReservationGuard<'_> {
    fn commit(mut self) {
        unsafe { self.store.commit(self.entity) };
        self.armed = false;
    }
}

impl Drop for ReservationGuard<'_> {
    fn drop(&mut self) {
        if self.armed {
            self.store.abandon(self.entity);
        }
    }
}

pub(crate) fn create_entity<T>(
    store: &dyn EntityStore,
    globals: &dyn GlobalStore,
    callbacks: &dyn CallbackStore,
    notified: &Cell<bool>,
    build: impl FnOnce(&mut Context<'_, T>) -> T,
) -> Result<Entity<T>, EntityAllocError>
where
    T: 'static,
{
    let reservation = store.reserve(Layout::new::<T>(), TypeId::of::<T>(), drop_value::<T>)?;

    let entity = Entity::from_id(reservation.id);

    let guard = ReservationGuard {
        store,
        entity: reservation.id,
        armed: true,
    };

    let mut cx = Context::from_parts(entity, store, globals, callbacks, notified);

    let value = build(&mut cx);

    unsafe { reservation.ptr.cast::<T>().as_ptr().write(value) };

    guard.commit();

    Ok(entity)
}
