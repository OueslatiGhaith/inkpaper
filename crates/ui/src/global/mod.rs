use core::{
    alloc::Layout,
    any::TypeId,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

#[cfg(feature = "alloc")]
use crate::slot_table::HeapSlots;
use crate::{
    BorrowKind, RuntimeCx,
    runtime::Source,
    slot_table::{FixedSlots, ReserveError, SlotStorage, SlotTable},
};

// unit tests run against the storage the `alloc` feature implies
#[cfg(all(test, not(feature = "alloc")))]
pub(crate) type TestGlobalArena<const BYTES: usize, const SLOTS: usize> =
    FixedGlobalArena<BYTES, SLOTS>;
#[cfg(all(test, feature = "alloc"))]
pub(crate) type TestGlobalArena<const BYTES: usize, const SLOTS: usize> = HeapGlobalArena<SLOTS>;

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
    UnsupportedAlignment {
        requested: usize,
        supported: usize,
    },
    BorrowConflict,
    /// heap-backed storage could not allocate
    AllocationFailed,
}

pub trait GlobalStore {
    fn acquire(
        &self,
        type_id: TypeId,
        kind: BorrowKind,
    ) -> Result<(usize, NonNull<u8>), GlobalAccessError>;
    fn release(&self, slot: usize, kind: BorrowKind);
}

impl From<ReserveError> for GlobalSetError {
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

/// fixed-capacity storage for application globals: at most `SLOTS` globals in `BYTES`
/// bytes. Never allocates.
pub type FixedGlobalArena<const BYTES: usize, const SLOTS: usize> =
    GlobalArena<FixedSlots<BYTES, SLOTS>>;

/// heap storage for application globals. The first allocation reserves `INITIAL_SLOTS`
/// entries.
#[cfg(feature = "alloc")]
pub type HeapGlobalArena<const INITIAL_SLOTS: usize = 0> = GlobalArena<HeapSlots<INITIAL_SLOTS>>;

/// application globals, at most one per type, stored by [`SlotTable`]
pub struct GlobalArena<S: SlotStorage> {
    slots: SlotTable<S, TypeId>,
}

impl<S: SlotStorage> Default for GlobalArena<S> {
    fn default() -> Self {
        Self {
            slots: SlotTable::default(),
        }
    }
}

impl<S: SlotStorage> GlobalArena<S> {
    pub(crate) fn len(&self) -> usize {
        self.slots.len()
    }

    pub(crate) fn used_bytes(&self) -> usize {
        self.slots.used_bytes()
    }

    pub(crate) fn capacity(&self) -> usize {
        self.slots.capacity()
    }

    pub(crate) fn byte_capacity(&self) -> usize {
        self.slots.byte_capacity()
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
            if self.slots.is_borrowed(slot) {
                return Err(GlobalSetError::BorrowConflict);
            }

            let (_, ptr) = self.slots.live(slot).expect("globals are always live");

            // SAFETY: the slot holds a G and no global guard borrows it. Install the
            // replacement before running the old value's destructor
            let old = unsafe { ptr.cast::<G>().as_ptr().replace(value) };
            drop(old);

            return Ok(());
        }

        let (slot, ptr) = self
            .slots
            .reserve(Layout::new::<G>(), type_id, drop_global::<G>)?;

        // SAFETY: the reservation fits G, and commit follows its initialization
        unsafe {
            ptr.cast::<G>().as_ptr().write(value);
            self.slots.commit(slot);
        }

        Ok(())
    }

    fn index_of(&self, type_id: TypeId) -> Option<usize> {
        self.slots.position(|slot_type| *slot_type == type_id)
    }
}

impl<S: SlotStorage> GlobalStore for GlobalArena<S> {
    fn acquire(
        &self,
        type_id: TypeId,
        kind: BorrowKind,
    ) -> Result<(usize, NonNull<u8>), GlobalAccessError> {
        let slot = self.index_of(type_id).ok_or(GlobalAccessError::NotFound)?;

        self.slots
            .acquire(slot, kind)
            .map_err(|_| GlobalAccessError::BorrowConflict)?;

        let (_, ptr) = self.slots.live(slot).expect("globals are always live");

        Ok((slot, ptr))
    }

    fn release(&self, slot: usize, kind: BorrowKind) {
        self.slots.release(slot, kind);
    }
}

impl<S: SlotStorage> crate::storage::GlobalStorage for GlobalArena<S> {
    fn set<G>(&mut self, value: G) -> Result<(), GlobalSetError>
    where
        G: Global,
    {
        GlobalArena::set(self, value)
    }

    fn contains<G>(&self) -> bool
    where
        G: Global,
    {
        GlobalArena::contains::<G>(self)
    }

    fn len(&self) -> usize {
        GlobalArena::len(self)
    }

    fn used_bytes(&self) -> usize {
        GlobalArena::used_bytes(self)
    }

    fn capacity(&self) -> usize {
        GlobalArena::capacity(self)
    }

    fn byte_capacity(&self) -> usize {
        GlobalArena::byte_capacity(self)
    }
}

unsafe fn drop_global<G>(ptr: *mut u8) {
    unsafe { core::ptr::drop_in_place(ptr.cast::<G>()) };
}

struct RawGlobalBorrow<'a> {
    store: &'a dyn GlobalStore,
    slot: usize,
    ptr: NonNull<u8>,
    kind: BorrowKind,
}

impl<'a> RawGlobalBorrow<'a> {
    fn acquire(
        store: &'a dyn GlobalStore,
        type_id: TypeId,
        kind: BorrowKind,
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
            borrow: RawGlobalBorrow::acquire(store, TypeId::of::<G>(), BorrowKind::Shared)?,
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
    runtime: RuntimeCx<'a>,
    _marker: PhantomData<&'a mut G>,
}

impl<'a, G> GlobalMut<'a, G>
where
    G: Global,
{
    /// borrows the global for changing it. A render that does so counts as reading it
    pub(crate) fn acquire(runtime: RuntimeCx<'a>) -> Result<Self, GlobalAccessError> {
        runtime.record_read(Source::Global(TypeId::of::<G>()));

        Ok(Self {
            borrow: RawGlobalBorrow::acquire(
                runtime.globals,
                TypeId::of::<G>(),
                BorrowKind::Exclusive,
            )?,
            runtime,
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
        self.runtime.global_changed(TypeId::of::<G>());
    }
}
