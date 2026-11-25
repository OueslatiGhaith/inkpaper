use core::{alloc::Layout, any::TypeId, ptr::NonNull};

use super::{Entity, EntityAccessError, EntityAllocError, EntityArena, EntityId};

use crate::{Context, RuntimeCx, slot_table::BorrowKind};

pub(crate) struct RawEntityReservation {
    pub(crate) id: EntityId,
    pub(crate) ptr: NonNull<u8>,
}

pub(crate) unsafe fn drop_value<T>(ptr: *mut u8) {
    unsafe { core::ptr::drop_in_place(ptr.cast::<T>()) };
}

pub(crate) struct RawEntityBorrow<'a> {
    store: &'a EntityArena,
    entity: EntityId,
    ptr: NonNull<u8>,
    kind: BorrowKind,
}

impl<'a> RawEntityBorrow<'a> {
    pub(crate) fn acquire(
        store: &'a EntityArena,
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

    pub(crate) fn ptr(&self) -> NonNull<u8> {
        self.ptr
    }
}

impl Drop for RawEntityBorrow<'_> {
    fn drop(&mut self) {
        self.store.release(self.entity, self.kind);
    }
}

struct ReservationGuard<'a> {
    store: &'a EntityArena,
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
