use core::{alloc::Layout, any::TypeId, ptr::NonNull};

use crate::{
    pool::{OutOfMemory, PoolRef, Table},
    slot_table::{BorrowKind, NotLive, ReserveError, SlotTable},
};

#[cfg(test)]
use super::{Entity, RawEntityBorrow, drop_value};
use super::{EntityId, EntityStore, RawEntityReservation};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityAllocError {
    /// every slot number is taken
    SlotsFull,
    OutOfMemory(OutOfMemory),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityAccessError {
    InvalidEntity,
    TypeMismatch,
    NotReady,
    BorrowConflict,
    /// the pool had no room to record a borrow that outlives the call, such as the
    /// reference [`Entity::read`](crate::Entity::read) returns
    OutOfMemory(OutOfMemory),
}

impl From<ReserveError> for EntityAllocError {
    fn from(error: ReserveError) -> Self {
        match error {
            ReserveError::SlotsFull => Self::SlotsFull,
            ReserveError::OutOfMemory(error) => Self::OutOfMemory(error),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct EntityMeta {
    type_id: TypeId,
    /// set by `notify` until the runtime takes the invalidation or renders the entity
    dirty: bool,
    /// notified before the runtime took the invalidation, and not rendered since
    pending: bool,
}

/// entity state, stored by [`SlotTable`].
///
/// entity ids always have generation 0: slots are never reused. A slot's [`TypeId`] is
/// checked before its value is borrowed.
pub(crate) struct EntityArena {
    slots: SlotTable<EntityMeta>,
}

impl EntityArena {
    pub(crate) fn new_in(pool: PoolRef) -> Self {
        Self {
            slots: SlotTable::new_in(pool, Table::Entities),
        }
    }
}

#[cfg(test)]
impl EntityArena {
    pub(crate) fn len(&self) -> usize {
        self.slots.len()
    }

    /// the bytes the entities and their slots take from the pool
    pub(crate) fn used_bytes(&self) -> usize {
        self.slots.used_bytes()
    }

    pub(crate) fn insert<T>(&self, value: T) -> Result<Entity<T>, EntityAllocError>
    where
        T: 'static,
    {
        let reservation = self.reserve(Layout::new::<T>(), TypeId::of::<T>(), drop_value::<T>)?;

        // SAFETY: the reservation fits T, and commit follows its initialization
        unsafe {
            reservation.ptr.cast::<T>().as_ptr().write(value);
            self.commit(reservation.id);
        }

        Ok(Entity::from_id(reservation.id))
    }

    pub(crate) fn read<T, R>(
        &self,
        entity: Entity<T>,
        f: impl FnOnce(&T) -> R,
    ) -> Result<R, EntityAccessError>
    where
        T: 'static,
    {
        let borrow = RawEntityBorrow::acquire(
            self,
            entity.entity_id(),
            TypeId::of::<T>(),
            BorrowKind::Shared,
        )?;

        // SAFETY: the guard validated T and holds a shared borrow until f returns
        Ok(f(unsafe { &*borrow.ptr().cast::<T>().as_ptr() }))
    }

    pub(crate) fn update<T, R>(
        &self,
        entity: Entity<T>,
        f: impl FnOnce(&mut T) -> R,
    ) -> Result<R, EntityAccessError>
    where
        T: 'static,
    {
        let borrow = RawEntityBorrow::acquire(
            self,
            entity.entity_id(),
            TypeId::of::<T>(),
            BorrowKind::Exclusive,
        )?;

        // SAFETY: the guard validated T and holds an exclusive borrow until f returns
        Ok(f(unsafe { &mut *borrow.ptr().cast::<T>().as_ptr() }))
    }
}

// SAFETY: values never move, type and state checks precede access, and every returned
// pointer has a tracked borrow. Reservations become accessible only through commit
unsafe impl EntityStore for EntityArena {
    fn reserve(
        &self,
        layout: Layout,
        type_id: TypeId,
        drop_fn: unsafe fn(*mut u8),
    ) -> Result<RawEntityReservation, EntityAllocError> {
        let meta = EntityMeta {
            type_id,
            dirty: false,
            pending: false,
        };
        let (slot, ptr) = self.slots.reserve(layout, meta, drop_fn)?;

        Ok(RawEntityReservation {
            id: EntityId::new(slot, 0),
            ptr,
        })
    }

    unsafe fn commit(&self, entity: EntityId) {
        debug_assert_eq!(entity.generation(), 0);

        // SAFETY: the caller initialized the reserved value
        unsafe { self.slots.commit(entity.slot()) };
    }

    fn abandon(&self, entity: EntityId) {
        if entity.generation() == 0 {
            self.slots.abandon(entity.slot());
        }
    }

    fn borrow(
        &self,
        entity: EntityId,
        type_id: TypeId,
        kind: BorrowKind,
    ) -> Result<NonNull<u8>, EntityAccessError> {
        if entity.generation() != 0 {
            return Err(EntityAccessError::InvalidEntity);
        }

        let slot = usize::from(entity.slot());
        let (meta, ptr) = self.slots.live(slot).map_err(|error| match error {
            NotLive::Initializing => EntityAccessError::NotReady,
            NotLive::Missing | NotLive::Abandoned | NotLive::Removed => {
                EntityAccessError::InvalidEntity
            }
        })?;

        if meta.type_id != type_id {
            return Err(EntityAccessError::TypeMismatch);
        }

        self.slots
            .acquire(slot, kind)
            .map_err(|_| EntityAccessError::BorrowConflict)?;

        Ok(ptr)
    }

    fn release(&self, entity: EntityId, kind: BorrowKind) {
        self.slots.release(usize::from(entity.slot()), kind);
    }

    fn mark_dirty(&self, entity: EntityId) {
        if entity.generation() == 0 {
            self.slots
                .update_meta(usize::from(entity.slot()), |meta| meta.dirty = true);
        }
    }

    #[cfg(test)]
    fn is_dirty(&self, entity: EntityId) -> bool {
        entity.generation() == 0
            && self
                .slots
                .meta(usize::from(entity.slot()))
                .is_some_and(|meta| meta.dirty)
    }

    fn has_dirty(&self) -> bool {
        self.slots.position(|meta| meta.dirty).is_some()
    }

    fn take_dirty(&self) {
        self.slots.for_each_meta_mut(|meta| {
            meta.pending |= meta.dirty;
            meta.dirty = false;
        });
    }

    fn needs_render(&self, entity: EntityId) -> bool {
        entity.generation() == 0
            && self
                .slots
                .meta(usize::from(entity.slot()))
                .is_some_and(|meta| meta.dirty || meta.pending)
    }

    fn has_pending_render(&self) -> bool {
        self.slots
            .position(|meta| meta.dirty || meta.pending)
            .is_some()
    }

    fn mark_rendered(&self, entity: EntityId) {
        if entity.generation() == 0 {
            self.slots.update_meta(usize::from(entity.slot()), |meta| {
                meta.dirty = false;
                meta.pending = false;
            });
        }
    }

    fn clear_pending(&self) {
        self.slots.for_each_meta_mut(|meta| meta.pending = false);
    }

    fn clear_dirty(&self) {
        self.slots.for_each_meta_mut(|meta| {
            meta.dirty = false;
            meta.pending = false;
        });
    }
}

#[cfg(test)]
#[path = "heap_tests.rs"]
mod heap_tests;

#[cfg(test)]
mod tests {
    use core::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::pool::testing::{pool_ref, pool_ref_of, pool_ref_with_room};

    struct Counter {
        value: i32,
    }

    struct Settings {
        volume: u8,
    }

    #[test]
    fn stores_heterogeneous_entities() {
        let arena = EntityArena::new_in(pool_ref());

        let counter = arena.insert(Counter { value: 42 }).unwrap();
        let settings = arena.insert(Settings { volume: 80 }).unwrap();

        assert_eq!(arena.read(counter, |counter| counter.value), Ok(42));
        assert_eq!(arena.read(settings, |settings| settings.volume), Ok(80));
    }

    #[test]
    fn update_entities() {
        let arena = EntityArena::new_in(pool_ref());

        let counter = arena.insert(Counter { value: 1 }).unwrap();

        arena.update(counter, |counter| counter.value += 1).unwrap();

        assert_eq!(arena.read(counter, |counter| counter.value), Ok(2));
    }

    #[test]
    fn allows_nested_shared_borrows() {
        let arena = EntityArena::new_in(pool_ref());

        let counter = arena.insert(Counter { value: 1 }).unwrap();

        arena
            .read(counter, |first| {
                arena
                    .read(counter, |second| {
                        assert_eq!(first.value, second.value);
                    })
                    .unwrap()
            })
            .unwrap();
    }

    #[test]
    fn rejects_update_during_read() {
        let arena = EntityArena::new_in(pool_ref());

        let counter = arena.insert(Counter { value: 1 }).unwrap();

        arena
            .read(counter, |_| {
                assert_eq!(
                    arena.update(counter, |_| {}),
                    Err(EntityAccessError::BorrowConflict)
                );
            })
            .unwrap();
    }

    #[test]
    fn rejects_access_during_update() {
        let arena = EntityArena::new_in(pool_ref());

        let counter = arena.insert(Counter { value: 1 }).unwrap();

        arena
            .update(counter, |_| {
                assert_eq!(
                    arena.read(counter, |_| {}),
                    Err(EntityAccessError::BorrowConflict)
                );
                assert_eq!(
                    arena.update(counter, |_| {}),
                    Err(EntityAccessError::BorrowConflict)
                );
            })
            .unwrap();
    }

    #[test]
    fn allows_nested_updates_of_different_entities() {
        let arena = EntityArena::new_in(pool_ref());

        let counter = arena.insert(Counter { value: 1 }).unwrap();
        let settings = arena.insert(Settings { volume: 10 }).unwrap();

        arena
            .update(counter, |counter| {
                arena
                    .update(settings, |settings| {
                        counter.value += 1;
                        settings.volume += 1;
                    })
                    .unwrap()
            })
            .unwrap();

        assert_eq!(arena.read(counter, |counter| counter.value), Ok(2));
        assert_eq!(arena.read(settings, |settings| settings.volume), Ok(11));
    }

    #[test]
    fn reports_slot_exhaustion() {
        struct Marker;

        // room for every slot. Zero-sized values take no memory of their own
        let arena = EntityArena::new_in(pool_ref_of(16 * 1024 * 1024));

        for _ in 0..=u16::MAX {
            arena.insert(Marker).unwrap();
        }

        assert!(matches!(
            arena.insert(Marker),
            Err(EntityAllocError::SlotsFull),
        ));
    }

    #[test]
    fn reports_storage_exhaustion() {
        struct Big {
            _data: [u8; 256],
        }

        let arena = EntityArena::new_in(pool_ref_with_room(128));

        assert!(matches!(
            arena.insert(Big { _data: [0; 256] }),
            Err(EntityAllocError::OutOfMemory(OutOfMemory {
                table: Table::Entities,
                ..
            })),
        ));
    }

    #[test]
    fn supports_highly_aligned_entities() {
        #[repr(align(64))]
        struct HighlyAligned {
            value: u8,
        }

        let arena = EntityArena::new_in(pool_ref());
        let entity = arena.insert(HighlyAligned { value: 1 }).unwrap();

        arena
            .read(entity, |aligned| {
                assert_eq!(core::ptr::from_ref(aligned).addr() % 64, 0);
                assert_eq!(aligned.value, 1);
            })
            .unwrap();
    }

    #[test]
    fn supports_zero_sized_entities() {
        struct Marker;

        let arena = EntityArena::new_in(pool_ref());

        let a = arena.insert(Marker).unwrap();
        let slots = arena.used_bytes();
        let b = arena.insert(Marker).unwrap();

        arena.read(a, |_| {}).unwrap();
        arena.read(b, |_| {}).unwrap();

        assert_eq!(arena.len(), 2);
        // only the slot list takes memory
        assert_eq!(arena.used_bytes(), slots);
    }

    #[test]
    fn detects_type_mismatches() {
        let arena = EntityArena::new_in(pool_ref());

        let counter = arena.insert(Counter { value: 1 }).unwrap();

        let fake = Entity::<Settings>::from_id(counter.entity_id());

        assert_eq!(
            arena.read(fake, |_| {}),
            Err(EntityAccessError::TypeMismatch)
        );
    }

    static DROPS: AtomicUsize = AtomicUsize::new(0);

    struct Droppable;

    impl Drop for Droppable {
        fn drop(&mut self) {
            DROPS.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn drops_stored_entities() {
        DROPS.store(0, Ordering::SeqCst);

        {
            let arena = EntityArena::new_in(pool_ref());

            arena.insert(Droppable).unwrap();
            arena.insert(Droppable).unwrap();

            assert_eq!(DROPS.load(Ordering::SeqCst), 0);
        }

        assert_eq!(DROPS.load(Ordering::SeqCst), 2);
    }
}
