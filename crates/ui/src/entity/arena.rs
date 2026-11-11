use core::{alloc::Layout, any::TypeId, ptr::NonNull};

use crate::{
    pool::{OutOfMemory, PoolRef, Table},
    slot_table::{BorrowKind, NotLive, RemoveError, ReserveError, SlotTable},
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
    /// the generation of the value in the slot. Ids carry it too, so an id only
    /// matches the value it was issued for
    generation: u16,
    /// the value was removed. Its slot may be reused, under the next generation
    removed: bool,
    type_id: TypeId,
    /// set by `notify` until the runtime takes the invalidation or renders the entity
    dirty: bool,
    /// notified before the runtime took the invalidation, and not rendered since
    pending: bool,
}

/// entity state, stored by [`SlotTable`].
///
/// removed slots are reused with their generation advanced, so ids of removed entities
/// never match a later one. A slot whose generation would wrap around is retired
/// instead. A slot's generation and [`TypeId`] are checked before its value is borrowed.
pub(crate) struct EntityArena {
    slots: SlotTable<EntityMeta>,
}

impl EntityArena {
    pub(crate) fn new_in(pool: PoolRef) -> Self {
        Self {
            slots: SlotTable::new_in(pool, Table::Entities),
        }
    }

    /// the metadata of the slot `entity` points to, if `entity` is still the value in it
    fn meta(&self, entity: EntityId) -> Option<EntityMeta> {
        self.slots
            .meta(usize::from(entity.slot()))
            .filter(|meta| meta.generation == entity.generation() && !meta.removed)
    }

    /// changes the metadata of the slot `entity` points to, if `entity` is still the
    /// value in it
    fn update_meta(&self, entity: EntityId, f: impl FnOnce(&mut EntityMeta)) {
        self.slots.update_meta(usize::from(entity.slot()), |meta| {
            if meta.generation == entity.generation() && !meta.removed {
                f(meta);
            }
        });
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
        let (slot, ptr) = self.slots.reserve_reusing_where(
            layout,
            // a slot whose generation would wrap around is retired
            |meta| meta.generation < u16::MAX,
            |removed| EntityMeta {
                generation: removed.map_or(0, |meta| meta.generation + 1),
                removed: false,
                type_id,
                dirty: false,
                pending: false,
            },
            drop_fn,
        )?;
        let generation = self
            .slots
            .meta(usize::from(slot))
            .expect("the slot was just reserved")
            .generation;

        Ok(RawEntityReservation {
            id: EntityId::new(slot, generation),
            ptr,
        })
    }

    unsafe fn commit(&self, entity: EntityId) {
        debug_assert!(self.meta(entity).is_some());

        // SAFETY: the caller initialized the reserved value
        unsafe { self.slots.commit(entity.slot()) };
    }

    fn abandon(&self, entity: EntityId) {
        if self.meta(entity).is_some() {
            self.slots.abandon(entity.slot());
        }
    }

    fn borrow(
        &self,
        entity: EntityId,
        type_id: TypeId,
        kind: BorrowKind,
    ) -> Result<NonNull<u8>, EntityAccessError> {
        let slot = usize::from(entity.slot());
        let (meta, ptr) = self.slots.live(slot).map_err(|error| match error {
            NotLive::Initializing => EntityAccessError::NotReady,
            NotLive::Missing | NotLive::Abandoned | NotLive::Removed => {
                EntityAccessError::InvalidEntity
            }
        })?;

        if meta.generation != entity.generation() {
            return Err(EntityAccessError::InvalidEntity);
        }

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

    fn remove(&self, entity: EntityId) -> Result<(), EntityAccessError> {
        if self.meta(entity).is_none() {
            return Err(EntityAccessError::InvalidEntity);
        }

        self.slots
            .remove(usize::from(entity.slot()))
            .map_err(|error| match error {
                RemoveError::NotLive => EntityAccessError::InvalidEntity,
                RemoveError::Borrowed => EntityAccessError::BorrowConflict,
            })?;

        // a removed entity no longer needs to render, and its id reaches nothing
        self.update_meta(entity, |meta| {
            meta.dirty = false;
            meta.pending = false;
            meta.removed = true;
        });

        Ok(())
    }

    fn mark_dirty(&self, entity: EntityId) {
        self.update_meta(entity, |meta| meta.dirty = true);
    }

    #[cfg(test)]
    fn is_dirty(&self, entity: EntityId) -> bool {
        self.meta(entity).is_some_and(|meta| meta.dirty)
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
        self.meta(entity)
            .is_some_and(|meta| meta.dirty || meta.pending)
    }

    fn has_pending_render(&self) -> bool {
        self.slots
            .position(|meta| meta.dirty || meta.pending)
            .is_some()
    }

    fn mark_rendered(&self, entity: EntityId) {
        self.update_meta(entity, |meta| {
            meta.dirty = false;
            meta.pending = false;
        });
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

    /// counts its drops in a shared cell
    struct Counted(std::rc::Rc<core::cell::Cell<u32>>);

    impl Drop for Counted {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    #[test]
    fn removing_an_entity_drops_it_and_its_id_stops_working() {
        let arena = EntityArena::new_in(pool_ref());
        let drops = std::rc::Rc::new(core::cell::Cell::new(0));
        let entity = arena.insert(Counted(drops.clone())).unwrap();
        let used = arena.used_bytes();

        assert_eq!(arena.remove(entity.entity_id()), Ok(()));

        assert_eq!(drops.get(), 1);
        assert!(arena.used_bytes() < used);
        assert_eq!(
            arena.read(entity, |_| ()),
            Err(EntityAccessError::InvalidEntity)
        );
        assert_eq!(
            arena.remove(entity.entity_id()),
            Err(EntityAccessError::InvalidEntity)
        );
    }

    #[test]
    fn a_borrowed_entity_is_not_removed() {
        let arena = EntityArena::new_in(pool_ref());
        let counter = arena.insert(Counter { value: 1 }).unwrap();

        arena
            .read(counter, |_| {
                assert_eq!(
                    arena.remove(counter.entity_id()),
                    Err(EntityAccessError::BorrowConflict)
                );
            })
            .unwrap();

        assert_eq!(arena.read(counter, |counter| counter.value), Ok(1));
    }

    #[test]
    fn a_removed_slot_is_reused_under_a_new_generation() {
        let arena = EntityArena::new_in(pool_ref());
        let old = arena.insert(Counter { value: 1 }).unwrap();
        arena.remove(old.entity_id()).unwrap();

        let new = arena.insert(Settings { volume: 7 }).unwrap();

        assert_eq!(new.entity_id().slot(), old.entity_id().slot());
        assert_ne!(new.entity_id(), old.entity_id());
        assert_eq!(arena.len(), 1);

        // the old id reaches neither the new value nor its type
        let old_as_new = Entity::<Settings>::from_id(old.entity_id());
        assert_eq!(
            arena.read(old_as_new, |_| ()),
            Err(EntityAccessError::InvalidEntity)
        );
        assert_eq!(arena.read(new, |settings| settings.volume), Ok(7));
    }

    #[test]
    fn a_removed_entity_no_longer_needs_to_render() {
        let arena = EntityArena::new_in(pool_ref());
        let entity = arena.insert(Counter { value: 1 }).unwrap();
        arena.mark_dirty(entity.entity_id());

        arena.remove(entity.entity_id()).unwrap();
        // marking a stale id does nothing
        arena.mark_dirty(entity.entity_id());

        assert!(!arena.has_dirty());
        assert!(!arena.needs_render(entity.entity_id()));
    }

    #[test]
    fn a_slot_is_retired_before_its_generation_wraps_around() {
        let arena = EntityArena::new_in(pool_ref());
        let mut entity = arena.insert(Counter { value: 0 }).unwrap();

        while entity.entity_id().generation() < u16::MAX {
            arena.remove(entity.entity_id()).unwrap();
            entity = arena.insert(Counter { value: 0 }).unwrap();

            assert_eq!(entity.entity_id().slot(), 0);
        }

        arena.remove(entity.entity_id()).unwrap();
        let next = arena.insert(Counter { value: 0 }).unwrap();

        assert_eq!(next.entity_id().slot(), 1);
        assert_eq!(next.entity_id().generation(), 0);
    }
}
