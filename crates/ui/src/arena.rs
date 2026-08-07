use core::{
    any::TypeId,
    cell::{Cell, RefCell, UnsafeCell},
    mem::MaybeUninit,
};

use heapless::Vec;

use crate::{Entity, EntityId};

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
    BorrowConflict,
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
enum BorrowState {
    Free,
    Shared(u16),
    Exclusive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BorrowKind {
    Shared,
    Exclusive,
}

struct BorrowGuard<'a> {
    state: &'a Cell<BorrowState>,
    kind: BorrowKind,
}

impl Drop for BorrowGuard<'_> {
    fn drop(&mut self) {
        match self.kind {
            BorrowKind::Exclusive => {
                debug_assert_eq!(self.state.get(), BorrowState::Exclusive);
                self.state.set(BorrowState::Free);
            }
            BorrowKind::Shared => match self.state.get() {
                BorrowState::Shared(1) => self.state.set(BorrowState::Free),
                BorrowState::Shared(count) if count > 1 => {
                    self.state.set(BorrowState::Shared(count - 1))
                }
                _ => debug_assert!(false, "invalid shared entity borrow state"),
            },
        }
    }
}

unsafe fn drop_value<T>(ptr: *mut u8) {
    unsafe { core::ptr::drop_in_place(ptr.cast::<T>()) };
}

fn align_up(value: usize, alignment: usize) -> Option<usize> {
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
/// 5. an `&mut T` exists only white that entity's [`BorrowState`] is `Exclusive`
/// 6. an `&T` never exists while that entity's [`BorrowState`] is `Exclusive`
/// 7. the runtime validates [`TypeId`] before converting raw storage into `&T`or `&mut T`
/// 8. no reference to the whole storage buffer is created while references to stored objects
///    may exist
/// 9. each inserted value is dropped once when the arena is dropped
pub struct EntityArena<const BYTES: usize, const SLOTS: usize> {
    storage: UnsafeCell<EntityStorage<BYTES>>,
    entries: RefCell<Vec<EntityMeta, SLOTS>>,
    borrows: [Cell<BorrowState>; SLOTS],
    cursor: Cell<usize>,
}

impl<const BYTES: usize, const SLOTS: usize> Default for EntityArena<BYTES, SLOTS> {
    fn default() -> Self {
        Self {
            storage: UnsafeCell::new(EntityStorage::new()),
            entries: RefCell::new(Vec::new()),
            borrows: core::array::from_fn(|_| Cell::new(BorrowState::Free)),
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
        let mut entries = self.entries.borrow_mut();
        let slot = entries.len();

        if slot >= SLOTS || slot > u16::MAX as usize {
            return Err(EntityAllocError::SlotsFull);
        }

        let alignment = align_of::<T>();
        if alignment > ENTITY_ARENA_ALIGNMENT {
            return Err(EntityAllocError::UnsupportedAlignment {
                requested: alignment,
                supported: ENTITY_ARENA_ALIGNMENT,
            });
        }

        let offset = align_up(self.cursor.get(), alignment).ok_or(EntityAllocError::StorageFull)?;

        // reserve at least one byte for ZST so separate entries still receive
        // distinct storage locations
        let allocation_size = size_of::<T>().max(1);

        let end = offset
            .checked_add(allocation_size)
            .ok_or(EntityAllocError::StorageFull)?;
        if end > BYTES {
            return Err(EntityAllocError::StorageFull);
        }

        let generation = 0;
        let meta = EntityMeta {
            offset,
            generation,
            type_id: TypeId::of::<T>(),
            drop_fn: drop_value::<T>,
        };

        // all fallible checks happen before T is moved into arena storage
        let ptr = unsafe { self.storage_ptr().add(offset).cast::<T>() };
        unsafe { ptr.write(value) };

        // we checked capacity above, so this should be impossible
        if entries.push(meta).is_err() {
            // we cannot simply return here because `value` has already been moved
            // into the arena. This represents an internal invariant violation
            unsafe { core::ptr::drop_in_place(ptr) };
            unreachable!("heapless entity metadata capacity changed unexpectedly");
        }

        self.cursor.set(end);

        Ok(Entity::from_id(EntityId::new(slot as u16, generation)))
    }

    fn typed_meta<T>(&self, entity: Entity<T>) -> Result<(usize, EntityMeta), EntityAccessError>
    where
        T: 'static,
    {
        let id = entity.entity_id();
        let slot = id.slot() as usize;

        let entries = self.entries.borrow();

        let meta = entries
            .get(slot)
            .copied()
            .ok_or(EntityAccessError::InvalidEntity)?;
        if meta.generation != id.generation() {
            return Err(EntityAccessError::InvalidEntity);
        }
        if meta.type_id != TypeId::of::<T>() {
            return Err(EntityAccessError::TypeMismatch);
        }

        Ok((slot, meta))
    }

    fn borrow_shared(&self, slot: usize) -> Result<BorrowGuard<'_>, EntityAccessError> {
        let state = self
            .borrows
            .get(slot)
            .ok_or(EntityAccessError::InvalidEntity)?;

        match state.get() {
            BorrowState::Free => state.set(BorrowState::Shared(1)),
            BorrowState::Shared(count) if count < u16::MAX => {
                state.set(BorrowState::Shared(count + 1))
            }
            _ => return Err(EntityAccessError::BorrowConflict),
        }

        Ok(BorrowGuard {
            state,
            kind: BorrowKind::Shared,
        })
    }

    fn borrow_exclusive(&self, slot: usize) -> Result<BorrowGuard<'_>, EntityAccessError> {
        let state = self
            .borrows
            .get(slot)
            .ok_or(EntityAccessError::InvalidEntity)?;

        match state.get() {
            BorrowState::Free => state.set(BorrowState::Exclusive),
            _ => return Err(EntityAccessError::BorrowConflict),
        }

        Ok(BorrowGuard {
            state,
            kind: BorrowKind::Exclusive,
        })
    }

    pub fn read<T, R>(
        &self,
        entity: Entity<T>,
        f: impl FnOnce(&T) -> R,
    ) -> Result<R, EntityAccessError>
    where
        T: 'static,
    {
        let (slot, meta) = self.typed_meta(entity)?;
        let _guard = self.borrow_shared(slot)?;

        let ptr = unsafe { self.storage_ptr().add(meta.offset).cast::<T>() };
        let value = unsafe { &*ptr };

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
        let (slot, meta) = self.typed_meta(entity)?;
        let _guard = self.borrow_exclusive(slot)?;

        let ptr = unsafe { self.storage_ptr().add(meta.offset).cast::<T>() };
        let value = unsafe { &mut *ptr };

        Ok(f(value))
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

        for meta in entries.iter().rev() {
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
    fn stores_hetergenous_entities() {
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
