use core::{alloc::Layout, any::TypeId, ptr::NonNull};

use super::{Entity, EntityAccessError, EntityAllocError, EntityId};

use crate::{Context, RuntimeCx, slot_table::BorrowKind};

pub struct RawEntityReservation {
    pub(crate) id: EntityId,
    pub(crate) ptr: NonNull<u8>,
}

pub unsafe trait EntityStore {
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
        kind: BorrowKind,
    ) -> Result<NonNull<u8>, EntityAccessError>;

    fn release(&self, entity: EntityId, kind: BorrowKind);

    /// records that the entity changed and needs to render again. Ignores invalid ids
    fn mark_dirty(&self, entity: EntityId);

    fn is_dirty(&self, entity: EntityId) -> bool;

    /// whether any entity is dirty
    fn has_dirty(&self) -> bool;

    fn clear_dirty(&self);
}

pub(crate) unsafe fn drop_value<T>(ptr: *mut u8) {
    unsafe { core::ptr::drop_in_place(ptr.cast::<T>()) };
}

pub(crate) struct RawEntityBorrow<'a> {
    store: &'a dyn EntityStore,
    entity: EntityId,
    ptr: NonNull<u8>,
    kind: BorrowKind,
}

impl<'a> RawEntityBorrow<'a> {
    pub fn acquire(
        store: &'a dyn EntityStore,
        entity: EntityId,
        type_id: TypeId,
        kind: BorrowKind,
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
    runtime: RuntimeCx<'_>,
    build: impl FnOnce(&mut Context<'_, T>) -> T,
) -> Result<Entity<T>, EntityAllocError>
where
    T: 'static,
{
    let store = runtime.entities;
    let reservation = store.reserve(Layout::new::<T>(), TypeId::of::<T>(), drop_value::<T>)?;

    let entity = Entity::from_id(reservation.id);

    let guard = ReservationGuard {
        store,
        entity: reservation.id,
        armed: true,
    };

    let mut cx = Context::new_in(entity, runtime);

    let value = build(&mut cx);

    unsafe { reservation.ptr.cast::<T>().as_ptr().write(value) };

    guard.commit();

    Ok(entity)
}
