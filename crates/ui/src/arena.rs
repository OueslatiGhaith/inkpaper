use core::{
    alloc::Layout,
    any::TypeId,
    cell::{Cell, RefCell, UnsafeCell},
    mem::MaybeUninit,
    ptr::NonNull,
};

use heapless::Vec;

use crate::{
    Entity, EntityId,
    entity_store::{BorrowState, EntityStore, RawEntityBorrow, RawEntityReservation, drop_value},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityAllocError {
    SlotsFull,
    StorageFull,
    UnsupportedAlignment { requested: usize, supported: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityAccessError {
    InvalidEntity,
    TypeMismatch,
    NotReady,
    BorrowConflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntitySlotState {
    Vacant,
    Initializing,
    Live,
    Abandoned,
}

const ENTITY_ARENA_ALIGNMENT: usize = 16;

#[repr(C, align(16))]
struct EntityStorage<const N: usize> {
    bytes: [MaybeUninit<u8>; N],
}

impl<const N: usize> EntityStorage<N> {
    fn new() -> Self {
        Self {
            bytes: [MaybeUninit::uninit(); N],
        }
    }
}

#[derive(Clone, Copy)]
struct EntityMeta {
    offset: usize,
    type_id: TypeId,
    generation: u16,
    drop_fn: unsafe fn(*mut u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EntityBorrowKind {
    Shared,
    Exclusive,
}

pub(crate) fn align_up(value: usize, alignment: usize) -> Option<usize> {
    debug_assert!(alignment.is_power_of_two());

    let mask = alignment - 1;
    value.checked_add(mask).map(|v| v & !mask)
}

/// SAFETY INVARIANTS:
///
/// 1. every [`EntityMeta`] refers to exactly one live value in `storage`
/// 2. the value at [`EntityMeta::offset`] has exactly the [`TypeId`] in that metadata
/// 3. allocated object ranges never overlap
/// 4. values never move after insertion
/// 5. an `&mut T` exists only while that entity's [`BorrowState`] is `Exclusive`
/// 6. an `&T` never exists while that entity's [`BorrowState`] is `Exclusive`
/// 7. the runtime validates [`TypeId`] before converting raw storage into `&T` or `&mut T`
/// 8. no reference to the whole storage buffer is created while references to stored objects
///    may exist
/// 9. each inserted value is dropped once when the arena is dropped
pub struct EntityArena<const BYTES: usize, const SLOTS: usize> {
    storage: UnsafeCell<EntityStorage<BYTES>>,
    entries: RefCell<Vec<EntityMeta, SLOTS>>,
    borrows: [Cell<BorrowState>; SLOTS],
    states: [Cell<EntitySlotState>; SLOTS],
    cursor: Cell<usize>,
}

impl<const BYTES: usize, const SLOTS: usize> Default for EntityArena<BYTES, SLOTS> {
    fn default() -> Self {
        Self {
            storage: UnsafeCell::new(EntityStorage::new()),
            entries: RefCell::new(Vec::new()),
            borrows: core::array::from_fn(|_| Cell::new(BorrowState::Free)),
            states: core::array::from_fn(|_| Cell::new(EntitySlotState::Vacant)),
            cursor: Cell::new(0),
        }
    }
}

impl<const BYTES: usize, const SLOTS: usize> EntityArena<BYTES, SLOTS> {
    pub fn len(&self) -> usize {
        self.entries.borrow().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub const fn capacity(&self) -> usize {
        SLOTS
    }

    pub fn used_bytes(&self) -> usize {
        self.cursor.get()
    }

    pub const fn byte_capacity(&self) -> usize {
        BYTES
    }

    fn storage_ptr(&self) -> *mut u8 {
        let storage = self.storage.get();

        unsafe {
            core::ptr::addr_of_mut!((*storage).bytes)
                .cast::<MaybeUninit<u8>>()
                .cast::<u8>()
        }
    }

    pub fn insert<T>(&self, value: T) -> Result<Entity<T>, EntityAllocError>
    where
        T: 'static,
    {
        let reservation = self.reserve(Layout::new::<T>(), TypeId::of::<T>(), drop_value::<T>)?;
        let entity = Entity::from_id(reservation.id);
        unsafe {
            reservation.ptr.cast::<T>().as_ptr().write(value);
            self.commit(reservation.id);
        }

        Ok(entity)
    }

    fn meta_for(
        &self,
        entity: EntityId,
        type_id: TypeId,
    ) -> Result<(usize, EntityMeta), EntityAccessError> {
        let slot = entity.slot() as usize;
        let entries = self.entries.borrow();

        let meta = entries
            .get(slot)
            .copied()
            .ok_or(EntityAccessError::InvalidEntity)?;

        if meta.generation != entity.generation() {
            return Err(EntityAccessError::InvalidEntity);
        }

        match self.states[slot].get() {
            EntitySlotState::Live => {}
            EntitySlotState::Initializing => return Err(EntityAccessError::NotReady),
            EntitySlotState::Vacant | EntitySlotState::Abandoned => {
                return Err(EntityAccessError::InvalidEntity);
            }
        }

        if meta.type_id != type_id {
            return Err(EntityAccessError::TypeMismatch);
        }

        Ok((slot, meta))
    }

    fn acquire_shared(&self, slot: usize) -> Result<(), EntityAccessError> {
        let state = self
            .borrows
            .get(slot)
            .ok_or(EntityAccessError::InvalidEntity)?;

        match state.get() {
            BorrowState::Free => {
                state.set(BorrowState::Shared(1));
                Ok(())
            }
            BorrowState::Shared(count) if count < u16::MAX => {
                state.set(BorrowState::Shared(count + 1));
                Ok(())
            }
            BorrowState::Shared(_) | BorrowState::Exclusive => {
                Err(EntityAccessError::BorrowConflict)
            }
        }
    }

    fn acquire_exclusive(&self, slot: usize) -> Result<(), EntityAccessError> {
        let state = self
            .borrows
            .get(slot)
            .ok_or(EntityAccessError::InvalidEntity)?;

        match state.get() {
            BorrowState::Free => {
                state.set(BorrowState::Exclusive);
                Ok(())
            }
            BorrowState::Shared(_) | BorrowState::Exclusive => {
                Err(EntityAccessError::BorrowConflict)
            }
        }
    }

    pub fn read<T, R>(
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
            EntityBorrowKind::Shared,
        )?;

        let value = unsafe { &*borrow.ptr().cast::<T>().as_ptr() };

        Ok(f(value))
    }

    pub fn update<T, R>(
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
            EntityBorrowKind::Exclusive,
        )?;

        let value = unsafe { &mut *borrow.ptr().cast::<T>().as_ptr() };

        Ok(f(value))
    }
}

unsafe impl<const BYTES: usize, const SLOTS: usize> EntityStore for EntityArena<BYTES, SLOTS> {
    fn reserve(
        &self,
        layout: Layout,
        type_id: TypeId,
        drop_fn: unsafe fn(*mut u8),
    ) -> Result<RawEntityReservation, EntityAllocError> {
        let mut entries = self.entries.borrow_mut();
        let slot = entries.len();

        if slot >= SLOTS || slot > u16::MAX as usize {
            return Err(EntityAllocError::SlotsFull);
        }

        let alignment = layout.align();
        if alignment > ENTITY_ARENA_ALIGNMENT {
            return Err(EntityAllocError::UnsupportedAlignment {
                requested: alignment,
                supported: ENTITY_ARENA_ALIGNMENT,
            });
        }

        let offset = align_up(self.cursor.get(), alignment).ok_or(EntityAllocError::StorageFull)?;

        // reserve at least one byte for ZST so separate entries still receive
        // distinct storage locations
        let allocation_size = layout.size().max(1);

        let end = offset
            .checked_add(allocation_size)
            .ok_or(EntityAllocError::StorageFull)?;
        if end > BYTES {
            return Err(EntityAllocError::StorageFull);
        }

        let generation = 0;
        let id = EntityId::new(slot as u16, generation);

        let meta = EntityMeta {
            offset,
            generation,
            type_id,
            drop_fn,
        };

        entries
            .push(meta)
            .map_err(|_| EntityAllocError::SlotsFull)?;

        self.cursor.set(end);
        self.states[slot].set(EntitySlotState::Initializing);

        let ptr = unsafe { self.storage_ptr().add(offset) };
        let ptr = unsafe { NonNull::new_unchecked(ptr) };

        Ok(RawEntityReservation { id, ptr })
    }

    unsafe fn commit(&self, entity: EntityId) {
        let slot = entity.slot() as usize;
        debug_assert_eq!(self.states[slot].get(), EntitySlotState::Initializing);
        self.states[slot].set(EntitySlotState::Live);
    }

    fn abandon(&self, entity: EntityId) {
        let slot = entity.slot() as usize;

        if matches!(self.states[slot].get(), EntitySlotState::Initializing) {
            self.states[slot].set(EntitySlotState::Abandoned);
        }
    }

    fn borrow(
        &self,
        entity: EntityId,
        type_id: TypeId,
        kind: EntityBorrowKind,
    ) -> Result<NonNull<u8>, EntityAccessError> {
        let (slot, meta) = self.meta_for(entity, type_id)?;

        match kind {
            EntityBorrowKind::Shared => self.acquire_shared(slot)?,
            EntityBorrowKind::Exclusive => self.acquire_exclusive(slot)?,
        }

        let ptr = unsafe { self.storage_ptr().add(meta.offset) };

        // # SAFETY
        //
        // - `storage_ptr` points to the arena's backing allocation, so it is non-null.
        // - `meta.offset` was produced by `reserve()` and validated against the arena's capacity
        // - `meta_for()` also verifies that this slot is `Live` and has the requested TypeId
        let ptr = unsafe { NonNull::new_unchecked(ptr) };

        Ok(ptr)
    }

    fn release(&self, entity: EntityId, kind: EntityBorrowKind) {
        let slot = entity.slot() as usize;
        let state = &self.borrows[slot];

        match kind {
            EntityBorrowKind::Exclusive => {
                debug_assert_eq!(state.get(), BorrowState::Exclusive);
                state.set(BorrowState::Free);
            }
            EntityBorrowKind::Shared => match state.get() {
                BorrowState::Shared(1) => state.set(BorrowState::Free),
                BorrowState::Shared(n) if n > 1 => state.set(BorrowState::Shared(n - 1)),
                _ => debug_assert!(false, "released entity without matching shared borrow"),
            },
        }
    }
}

impl<const BYTES: usize, const SLOTS: usize> Drop for EntityArena<BYTES, SLOTS> {
    fn drop(&mut self) {
        let storage = self.storage.get();

        let storage_ptr = unsafe {
            core::ptr::addr_of_mut!((*storage).bytes)
                .cast::<MaybeUninit<u8>>()
                .cast::<u8>()
        };

        let entries = self.entries.get_mut();

        for (slot, meta) in entries.iter().enumerate().rev() {
            if self.states[slot].get() != EntitySlotState::Live {
                continue;
            }

            let ptr = unsafe { storage_ptr.add(meta.offset) };
            unsafe { (meta.drop_fn)(ptr) }
        }
    }
}

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
        let arena = EntityArena::<1024, 16>::default();

        let counter = arena.insert(Counter { value: 42 }).unwrap();
        let settings = arena.insert(Settings { volume: 80 }).unwrap();

        assert_eq!(arena.read(counter, |counter| counter.value), Ok(42));
        assert_eq!(arena.read(settings, |settings| settings.volume), Ok(80));
    }

    #[test]
    fn update_entities() {
        let arena = EntityArena::<1024, 4>::default();

        let counter = arena.insert(Counter { value: 1 }).unwrap();

        arena.update(counter, |counter| counter.value += 1).unwrap();

        assert_eq!(arena.read(counter, |counter| counter.value), Ok(2));
    }

    #[test]
    fn allows_nested_shared_borrows() {
        let arena = EntityArena::<256, 4>::default();

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
        let arena = EntityArena::<256, 4>::default();

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
        let arena = EntityArena::<256, 4>::default();

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
        let arena = EntityArena::<256, 4>::default();

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
        let arena = EntityArena::<256, 1>::default();

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

        let arena = EntityArena::<32, 4>::default();

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

        let arena = EntityArena::<256, 4>::default();

        assert!(matches!(
            arena.insert(HighlyAligned { _value: 1 }),
            Err(EntityAllocError::UnsupportedAlignment { .. }),
        ));
    }

    #[test]
    fn supports_zero_sized_entities() {
        struct Marker;

        let arena = EntityArena::<8, 4>::default();

        let a = arena.insert(Marker).unwrap();
        let b = arena.insert(Marker).unwrap();

        arena.read(a, |_| {}).unwrap();
        arena.read(b, |_| {}).unwrap();

        assert_eq!(arena.len(), 2);
        assert_eq!(arena.used_bytes(), 2);
    }

    #[test]
    fn detects_type_mismatches() {
        let arena = EntityArena::<256, 4>::default();

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
            let arena = EntityArena::<256, 4>::default();

            arena.insert(Droppable).unwrap();
            arena.insert(Droppable).unwrap();

            assert_eq!(DROPS.load(Ordering::SeqCst), 0);
        }

        assert_eq!(DROPS.load(Ordering::SeqCst), 2);
    }
}
