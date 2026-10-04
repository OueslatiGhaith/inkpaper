use core::{alloc::Layout, any::TypeId, ptr::NonNull};

#[cfg(feature = "alloc")]
use crate::slot_table::HeapSlots;
use crate::slot_table::{BorrowKind, FixedSlots, NotLive, ReserveError, SlotStorage, SlotTable};

use super::{Entity, EntityId, EntityStore, RawEntityBorrow, RawEntityReservation, drop_value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityAllocError {
    SlotsFull,
    StorageFull,
    UnsupportedAlignment {
        requested: usize,
        supported: usize,
    },
    /// heap-backed storage could not allocate
    AllocationFailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityAccessError {
    InvalidEntity,
    TypeMismatch,
    NotReady,
    BorrowConflict,
}

impl From<ReserveError> for EntityAllocError {
    fn from(error: ReserveError) -> Self {
        match error {
            ReserveError::SlotsFull => Self::SlotsFull,
            ReserveError::StorageFull => Self::StorageFull,
            ReserveError::UnsupportedAlignment {
                requested,
                supported,
            } => Self::UnsupportedAlignment {
                requested,
                supported,
            },
            ReserveError::AllocationFailed => Self::AllocationFailed,
        }
    }
}

/// fixed-capacity entity storage: at most `SLOTS` entities in `BYTES` bytes. Never
/// allocates.
pub type FixedEntityArena<const BYTES: usize, const SLOTS: usize> =
    EntityArena<FixedSlots<BYTES, SLOTS>>;

/// heap entity storage. The first allocation reserves `INITIAL_SLOTS` entries.
#[cfg(feature = "alloc")]
pub type HeapEntityArena<const INITIAL_SLOTS: usize = 0> = EntityArena<HeapSlots<INITIAL_SLOTS>>;

/// entity state, stored by [`SlotTable`].
///
/// entity ids always have generation 0: slots are never reused. A slot's [`TypeId`] is
/// checked before its value is borrowed.
pub struct EntityArena<S: SlotStorage> {
    slots: SlotTable<S, TypeId>,
}

impl<S: SlotStorage> Default for EntityArena<S> {
    fn default() -> Self {
        Self {
            slots: SlotTable::default(),
        }
    }
}

impl<S: SlotStorage> EntityArena<S> {
    pub(crate) fn len(&self) -> usize {
        self.slots.len()
    }

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
unsafe impl<S: SlotStorage> EntityStore for EntityArena<S> {
    fn reserve(
        &self,
        layout: Layout,
        type_id: TypeId,
        drop_fn: unsafe fn(*mut u8),
    ) -> Result<RawEntityReservation, EntityAllocError> {
        let (slot, ptr) = self.slots.reserve(layout, type_id, drop_fn)?;

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
        let (slot_type, ptr) = self.slots.live(slot).map_err(|error| match error {
            NotLive::Initializing => EntityAccessError::NotReady,
            NotLive::Missing | NotLive::Abandoned => EntityAccessError::InvalidEntity,
        })?;

        if slot_type != type_id {
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
}

impl<S: SlotStorage> crate::storage::EntityStorage for EntityArena<S> {}

#[cfg(all(test, feature = "alloc"))]
#[path = "heap_tests.rs"]
mod heap_tests;

#[cfg(test)]
mod tests {
    use core::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    struct Counter {
        value: i32,
    }

    struct Settings {
        volume: u8,
    }

    #[test]
    fn stores_heterogeneous_entities() {
        let arena = FixedEntityArena::<1024, 16>::default();

        let counter = arena.insert(Counter { value: 42 }).unwrap();
        let settings = arena.insert(Settings { volume: 80 }).unwrap();

        assert_eq!(arena.read(counter, |counter| counter.value), Ok(42));
        assert_eq!(arena.read(settings, |settings| settings.volume), Ok(80));
    }

    #[test]
    fn update_entities() {
        let arena = FixedEntityArena::<1024, 4>::default();

        let counter = arena.insert(Counter { value: 1 }).unwrap();

        arena.update(counter, |counter| counter.value += 1).unwrap();

        assert_eq!(arena.read(counter, |counter| counter.value), Ok(2));
    }

    #[test]
    fn allows_nested_shared_borrows() {
        let arena = FixedEntityArena::<256, 4>::default();

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
        let arena = FixedEntityArena::<256, 4>::default();

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
        let arena = FixedEntityArena::<256, 4>::default();

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
        let arena = FixedEntityArena::<256, 4>::default();

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
        let arena = FixedEntityArena::<256, 1>::default();

        arena.insert(Counter { value: 1 }).unwrap();

        assert!(matches!(
            arena.insert(Counter { value: 2 }),
            Err(EntityAllocError::SlotsFull),
        ));
    }

    #[test]
    fn reports_storage_exhaustion() {
        struct Big {
            _data: [u8; 64],
        }

        let arena = FixedEntityArena::<32, 4>::default();

        assert!(matches!(
            arena.insert(Big { _data: [0; 64] }),
            Err(EntityAllocError::StorageFull),
        ));
    }

    #[test]
    fn rejects_unsupported_alignment() {
        #[repr(align(32))]
        struct HighlyAligned {
            _value: u8,
        }

        let arena = FixedEntityArena::<256, 4>::default();

        assert!(matches!(
            arena.insert(HighlyAligned { _value: 1 }),
            Err(EntityAllocError::UnsupportedAlignment { .. }),
        ));
    }

    #[test]
    fn supports_zero_sized_entities() {
        struct Marker;

        let arena = FixedEntityArena::<8, 4>::default();

        let a = arena.insert(Marker).unwrap();
        let b = arena.insert(Marker).unwrap();

        arena.read(a, |_| {}).unwrap();
        arena.read(b, |_| {}).unwrap();

        assert_eq!(arena.len(), 2);
        assert_eq!(arena.used_bytes(), 2);
    }

    #[test]
    fn detects_type_mismatches() {
        let arena = FixedEntityArena::<256, 4>::default();

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
            let arena = FixedEntityArena::<256, 4>::default();

            arena.insert(Droppable).unwrap();
            arena.insert(Droppable).unwrap();

            assert_eq!(DROPS.load(Ordering::SeqCst), 0);
        }

        assert_eq!(DROPS.load(Ordering::SeqCst), 2);
    }
}
