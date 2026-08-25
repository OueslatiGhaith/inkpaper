use core::{
    any::TypeId,
    cell::{Cell, UnsafeCell},
    marker::PhantomData,
    mem::MaybeUninit,
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

use heapless::Vec;

use crate::align_up;

/// marker trait for application-wide immutable values
///
/// globals are intended for configuration and application-wide resources such as themes,
/// locale information, or device capabilities
///
/// mutable application state should normally live in a [`Entity`](crate::Entity) instead.
pub trait Global: 'static {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum GlobalAccessError {
    NotFound,
    BorrowConflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum GlobalSetError {
    SlotsFull,
    StorageFull,
    UnsupportedAlignment { requested: usize, supported: usize },
    BorrowConflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GlobalBorrowKind {
    Shared,
    Exclusive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GlobalBorrowState {
    Free,
    Shared(u16),
    Exclusive,
}

#[derive(Clone, Copy)]
struct GlobalMeta {
    type_id: TypeId,
    offset: usize,
    drop_fn: unsafe fn(*mut u8),
}

const GLOBAL_ARENA_ALIGNMENT: usize = 16;

#[repr(C, align(16))]
struct GlobalStorage<const N: usize> {
    bytes: [MaybeUninit<u8>; N],
}

impl<const N: usize> Default for GlobalStorage<N> {
    fn default() -> Self {
        Self {
            bytes: [MaybeUninit::uninit(); N],
        }
    }
}

pub(crate) trait GlobalStore {
    fn acquire(
        &self,
        type_id: TypeId,
        kind: GlobalBorrowKind,
    ) -> Result<(usize, NonNull<u8>), GlobalAccessError>;
    fn release(&self, slot: usize, kind: GlobalBorrowKind);
}

/// fixed-capacity storage for application globals
pub(crate) struct GlobalArena<const BYTES: usize, const SLOTS: usize> {
    storage: UnsafeCell<GlobalStorage<BYTES>>,
    entries: Vec<GlobalMeta, SLOTS>,
    borrows: [Cell<GlobalBorrowState>; SLOTS],
    cursor: usize,
}

impl<const BYTES: usize, const SLOTS: usize> Default for GlobalArena<BYTES, SLOTS> {
    fn default() -> Self {
        Self {
            storage: UnsafeCell::new(GlobalStorage::default()),
            entries: Vec::new(),
            borrows: core::array::from_fn(|_| Cell::new(GlobalBorrowState::Free)),
            cursor: 0,
        }
    }
}

impl<const BYTES: usize, const SLOTS: usize> GlobalArena<BYTES, SLOTS> {
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(crate) fn used_bytes(&self) -> usize {
        self.cursor
    }

    pub(crate) const fn capacity(&self) -> usize {
        SLOTS
    }

    pub(crate) const fn byte_capacity(&self) -> usize {
        BYTES
    }

    pub(crate) fn contains<G>(&self) -> bool
    where
        G: Global,
    {
        self.index_of(TypeId::of::<G>()).is_some()
    }

    pub(crate) fn set<G>(&mut self, value: G) -> Result<(), GlobalSetError>
    where
        G: Global,
    {
        let type_id = TypeId::of::<G>();
        if let Some(slot) = self.index_of(type_id) {
            if self.borrows[slot].get() != GlobalBorrowState::Free {
                return Err(GlobalSetError::BorrowConflict);
            }

            let meta = self.entries[slot];
            let ptr = unsafe { self.storage_ptr().add(meta.offset) };
            let old = unsafe { ptr.cast::<G>().replace(value) };

            drop(old);

            return Ok(());
        }

        if self.entries.len() >= SLOTS {
            return Err(GlobalSetError::SlotsFull);
        }

        let layout = core::alloc::Layout::new::<G>();
        let alignment = layout.align();
        if alignment > GLOBAL_ARENA_ALIGNMENT {
            return Err(GlobalSetError::UnsupportedAlignment {
                requested: alignment,
                supported: GLOBAL_ARENA_ALIGNMENT,
            });
        }

        let offset = align_up(self.cursor, alignment).ok_or(GlobalSetError::StorageFull)?;

        // reserve one byte for ZSTs so entries still get distinct storage locations.
        let allocation_size = layout.size().max(1);

        let end = offset
            .checked_add(allocation_size)
            .ok_or(GlobalSetError::StorageFull)?;

        if end > BYTES {
            return Err(GlobalSetError::StorageFull);
        }

        let meta = GlobalMeta {
            type_id,
            offset,
            drop_fn: drop_global::<G>,
        };

        self.entries
            .push(meta)
            .map_err(|_| GlobalSetError::SlotsFull)?;

        let ptr = unsafe { self.storage_ptr().add(offset) };

        unsafe {
            ptr.cast::<G>().write(value);
        }

        self.cursor = end;

        Ok(())
    }

    fn index_of(&self, type_id: TypeId) -> Option<usize> {
        self.entries
            .iter()
            .position(|entry| entry.type_id == type_id)
    }

    fn storage_ptr(&self) -> *mut u8 {
        let storage = self.storage.get();

        unsafe {
            core::ptr::addr_of_mut!((*storage).bytes)
                .cast::<MaybeUninit<u8>>()
                .cast::<u8>()
        }
    }

    fn acquire_shared(&self, slot: usize) -> Result<(), GlobalAccessError> {
        let state = &self.borrows[slot];

        match state.get() {
            GlobalBorrowState::Free => state.set(GlobalBorrowState::Shared(1)),
            GlobalBorrowState::Shared(count) if count < u16::MAX => {
                state.set(GlobalBorrowState::Shared(count + 1));
            }
            GlobalBorrowState::Shared(_) | GlobalBorrowState::Exclusive => {
                return Err(GlobalAccessError::BorrowConflict);
            }
        }

        Ok(())
    }

    fn acquire_exclusive(&self, slot: usize) -> Result<(), GlobalAccessError> {
        let state = &self.borrows[slot];

        match state.get() {
            GlobalBorrowState::Free => state.set(GlobalBorrowState::Exclusive),
            GlobalBorrowState::Shared(_) | GlobalBorrowState::Exclusive => {
                return Err(GlobalAccessError::BorrowConflict);
            }
        }

        Ok(())
    }
}

impl<const BYTES: usize, const SLOTS: usize> GlobalStore for GlobalArena<BYTES, SLOTS> {
    fn acquire(
        &self,
        type_id: TypeId,
        kind: GlobalBorrowKind,
    ) -> Result<(usize, NonNull<u8>), GlobalAccessError> {
        let slot = self.index_of(type_id).ok_or(GlobalAccessError::NotFound)?;
        match kind {
            GlobalBorrowKind::Shared => self.acquire_shared(slot)?,
            GlobalBorrowKind::Exclusive => self.acquire_exclusive(slot)?,
        }

        let meta = self.entries[slot];
        let ptr = unsafe { self.storage_ptr().add(meta.offset) };
        let ptr = unsafe { NonNull::new_unchecked(ptr) };

        Ok((slot, ptr))
    }

    fn release(&self, slot: usize, kind: GlobalBorrowKind) {
        let state = &self.borrows[slot];

        match kind {
            GlobalBorrowKind::Exclusive => {
                debug_assert_eq!(state.get(), GlobalBorrowState::Exclusive);
                state.set(GlobalBorrowState::Free);
            }
            GlobalBorrowKind::Shared => match state.get() {
                GlobalBorrowState::Shared(1) => state.set(GlobalBorrowState::Free),
                GlobalBorrowState::Shared(count) if count > 1 => {
                    state.set(GlobalBorrowState::Shared(count - 1));
                }
                _ => debug_assert!(false, "released global without matching shared borrow"),
            },
        }
    }
}

impl<const BYTES: usize, const SLOTS: usize> Drop for GlobalArena<BYTES, SLOTS> {
    fn drop(&mut self) {
        let storage_ptr = self.storage_ptr();

        for meta in self.entries.iter().rev() {
            let ptr = unsafe { storage_ptr.add(meta.offset) };
            unsafe { (meta.drop_fn)(ptr) };
        }
    }
}

unsafe fn drop_global<G>(ptr: *mut u8) {
    unsafe { core::ptr::drop_in_place(ptr.cast::<G>()) };
}

struct RawGlobalBorrow<'a> {
    store: &'a dyn GlobalStore,
    slot: usize,
    ptr: NonNull<u8>,
    kind: GlobalBorrowKind,
}

impl<'a> RawGlobalBorrow<'a> {
    fn acquire(
        store: &'a dyn GlobalStore,
        type_id: TypeId,
        kind: GlobalBorrowKind,
    ) -> Result<Self, GlobalAccessError> {
        let (slot, ptr) = store.acquire(type_id, kind)?;

        Ok(Self {
            store,
            slot,
            ptr,
            kind,
        })
    }

    fn ptr(&self) -> NonNull<u8> {
        self.ptr
    }
}

impl Drop for RawGlobalBorrow<'_> {
    fn drop(&mut self) {
        self.store.release(self.slot, self.kind);
    }
}

pub struct GlobalRef<'a, G>
where
    G: Global,
{
    borrow: RawGlobalBorrow<'a>,
    _marker: PhantomData<&'a G>,
}

impl<'a, G> GlobalRef<'a, G>
where
    G: Global,
{
    pub(crate) fn acquire(store: &'a dyn GlobalStore) -> Result<Self, GlobalAccessError> {
        Ok(Self {
            borrow: RawGlobalBorrow::acquire(store, TypeId::of::<G>(), GlobalBorrowKind::Shared)?,
            _marker: PhantomData,
        })
    }
}

impl<G> Deref for GlobalRef<'_, G>
where
    G: Global,
{
    type Target = G;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.borrow.ptr().cast::<G>().as_ptr() }
    }
}

pub struct GlobalMut<'a, G>
where
    G: Global,
{
    borrow: RawGlobalBorrow<'a>,
    notified: &'a Cell<bool>,
    _marker: PhantomData<&'a mut G>,
}

impl<'a, G> GlobalMut<'a, G>
where
    G: Global,
{
    pub(crate) fn acquire(
        store: &'a dyn GlobalStore,
        notified: &'a Cell<bool>,
    ) -> Result<Self, GlobalAccessError> {
        Ok(Self {
            borrow: RawGlobalBorrow::acquire(
                store,
                TypeId::of::<G>(),
                GlobalBorrowKind::Exclusive,
            )?,
            notified,
            _marker: PhantomData,
        })
    }
}

impl<G> Deref for GlobalMut<'_, G>
where
    G: Global,
{
    type Target = G;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.borrow.ptr().cast::<G>().as_ptr() }
    }
}

impl<G> DerefMut for GlobalMut<'_, G>
where
    G: Global,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.borrow.ptr().cast::<G>().as_ptr() }
    }
}

impl<G> Drop for GlobalMut<'_, G>
where
    G: Global,
{
    fn drop(&mut self) {
        self.notified.set(true);
    }
}
