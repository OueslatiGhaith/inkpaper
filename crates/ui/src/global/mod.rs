use core::{
    alloc::Layout,
    any::TypeId,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

use crate::{
    BorrowKind, RuntimeCx,
    pool::{OutOfMemory, PoolRef, Table},
    runtime::Source,
    slot_table::{ReserveError, SlotTable},
};

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
    /// every slot number is taken
    SlotsFull,
    OutOfMemory(OutOfMemory),
    BorrowConflict,
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
            ReserveError::OutOfMemory(error) => Self::OutOfMemory(error),
        }
    }
}

/// application globals, at most one per type, stored by [`SlotTable`]
pub(crate) struct GlobalArena {
    slots: SlotTable<TypeId>,
}

impl GlobalArena {
    pub(crate) fn new_in(pool: PoolRef) -> Self {
        Self {
            slots: SlotTable::new_in(pool, Table::Globals),
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.slots.len()
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

impl GlobalStore for GlobalArena {
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
