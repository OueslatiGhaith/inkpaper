//! the slot table shared by the entity, callback and global arenas.
//!
//! a [`SlotTable`] stores type-erased values. Each slot records the value's memory in the
//! runtime's pool, arena-specific metadata, the value's destructor, its lifecycle state
//! and its borrow state. Every value has its own allocation, released when the value is
//! removed.
//!
//! SAFETY INVARIANTS:
//!
//! 1. a slot's memory is never shared with another slot and never moves, so values never
//!    overlap. Growing the slot list moves slots, not values.
//! 2. only `Live` slots hold an initialized value. Each is dropped exactly once: when it
//!    is removed, or when the table is cleared or dropped, in reverse reservation order.
//! 3. no `RefCell` guard on the slot list is held while user code runs, so callbacks may
//!    reserve new slots.
//! 4. borrow states only count borrows. Arenas pair each `acquire` with one `release`
//!    and convert pointers into references only while the matching borrow is held.

use core::{alloc::Layout, cell::RefCell, ptr::NonNull};

use crate::{
    pool::{PoolRef, PoolVec, Table},
    storage::VecStorage,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReserveError {
    /// every slot number is taken
    SlotsFull,
    /// the pool has no room for the value or the slot
    StorageFull,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorrowKind {
    Shared,
    Exclusive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BorrowState {
    Free,
    Shared(u16),
    Exclusive,
}

impl BorrowState {
    fn acquire(self, kind: BorrowKind) -> Option<Self> {
        match (kind, self) {
            (BorrowKind::Shared, Self::Free) => Some(Self::Shared(1)),
            (BorrowKind::Shared, Self::Shared(count)) if count < u16::MAX => {
                Some(Self::Shared(count + 1))
            }
            (BorrowKind::Exclusive, Self::Free) => Some(Self::Exclusive),
            _ => None,
        }
    }

    fn release(self, kind: BorrowKind) -> Self {
        match (kind, self) {
            (BorrowKind::Exclusive, Self::Exclusive) | (BorrowKind::Shared, Self::Shared(1)) => {
                Self::Free
            }
            (BorrowKind::Shared, Self::Shared(count)) if count > 1 => Self::Shared(count - 1),
            _ => {
                debug_assert!(false, "released a slot without a matching borrow");
                self
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SlotState {
    Initializing,
    Live,
    Abandoned,
    Removed,
}

/// why a slot has no live value
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NotLive {
    Missing,
    Initializing,
    Abandoned,
    Removed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BorrowConflict;

struct Slot<M> {
    /// the value's memory in the pool, until it is released
    memory: Option<NonNull<u8>>,
    layout: Layout,
    meta: M,
    drop_fn: unsafe fn(*mut u8),
    state: SlotState,
    borrow: BorrowState,
}

impl<M> Slot<M> {
    fn ptr(&self) -> NonNull<u8> {
        self.memory.expect("only slots with a value are read")
    }
}

/// type-erased values with per-slot metadata `M`, in the runtime's pool
pub(crate) struct SlotTable<M> {
    pool: PoolRef,
    /// the table the values count under
    table: Table,
    slots: RefCell<PoolVec<Slot<M>>>,
}

impl<M> SlotTable<M> {
    pub(crate) fn new_in(pool: PoolRef, table: Table) -> Self {
        Self {
            pool,
            table,
            slots: RefCell::new(PoolVec::new_in(pool, table)),
        }
    }

    fn allocate(&self, layout: Layout) -> Result<NonNull<u8>, ReserveError> {
        self.pool
            .allocate(self.table, layout)
            .ok_or(ReserveError::StorageFull)
    }

    /// returns a slot's memory to the pool, if it still holds any
    fn release_memory(pool: PoolRef, table: Table, slot: &mut Slot<M>) {
        if let Some(memory) = slot.memory.take() {
            // SAFETY: the memory came from this pool for the slot's layout, and the
            // slot no longer points to it
            unsafe { pool.deallocate(table, memory, slot.layout) };
        }
    }
}

impl<M: Copy> SlotTable<M> {
    pub(crate) fn len(&self) -> usize {
        self.slots.borrow().len()
    }

    #[cfg(test)]
    pub(crate) fn capacity(&self) -> usize {
        self.slots.borrow().capacity()
    }

    /// the bytes the values and their slots take from the pool
    #[cfg(test)]
    pub(crate) fn used_bytes(&self) -> usize {
        self.pool.stats().table(self.table)
    }

    /// reserves an `Initializing` slot whose memory fits `layout`. Slot numbers fit in a
    /// `u16`.
    pub(crate) fn reserve(
        &self,
        layout: Layout,
        meta: M,
        drop_fn: unsafe fn(*mut u8),
    ) -> Result<(u16, NonNull<u8>), ReserveError> {
        let mut slots = self.slots.borrow_mut();
        let slot = u16::try_from(slots.len()).map_err(|_| ReserveError::SlotsFull)?;

        // reserve the slot first, so a failed slot list doesn't leave a value behind
        slots
            .try_reserve(1)
            .map_err(|_| ReserveError::StorageFull)?;
        let ptr = self.allocate(layout)?;

        slots
            .try_push(Slot {
                memory: Some(ptr),
                layout,
                meta,
                drop_fn,
                state: SlotState::Initializing,
                borrow: BorrowState::Free,
            })
            .unwrap_or_else(|_| unreachable!("slot was reserved"));

        Ok((slot, ptr))
    }

    /// reserves like [`Self::reserve`], but in a removed slot when there is one. The
    /// caller must make sure handles to the removed value can't match the new one
    pub(crate) fn reserve_reusing(
        &self,
        layout: Layout,
        meta: M,
        drop_fn: unsafe fn(*mut u8),
    ) -> Result<(u16, NonNull<u8>), ReserveError> {
        let mut slots = self.slots.borrow_mut();
        let Some(index) = (0..=u16::MAX)
            .zip(slots.iter())
            .find(|(_, slot)| slot.state == SlotState::Removed)
            .map(|(index, _)| index)
        else {
            drop(slots);
            return self.reserve(layout, meta, drop_fn);
        };

        let ptr = self.allocate(layout)?;
        let slot = &mut slots[usize::from(index)];
        slot.memory = Some(ptr);
        slot.layout = layout;
        slot.meta = meta;
        slot.drop_fn = drop_fn;
        slot.state = SlotState::Initializing;
        slot.borrow = BorrowState::Free;

        Ok((index, ptr))
    }

    /// marks a reserved slot's value as initialized
    ///
    /// # Safety
    ///
    /// the slot's memory must hold a valid value matching its `drop_fn`
    pub(crate) unsafe fn commit(&self, slot: u16) {
        let mut slots = self.slots.borrow_mut();
        let slot = &mut slots[usize::from(slot)];

        debug_assert_eq!(slot.state, SlotState::Initializing);
        slot.state = SlotState::Live;
    }

    /// gives up a reserved slot whose value was never initialized. Its slot number stays
    /// taken, so handles to it remain invalid.
    pub(crate) fn abandon(&self, slot: u16) {
        let mut slots = self.slots.borrow_mut();

        let Some(slot) = slots.get_mut(usize::from(slot)) else {
            return;
        };

        if slot.state == SlotState::Initializing {
            slot.state = SlotState::Abandoned;
            Self::release_memory(self.pool, self.table, slot);
        }
    }

    /// the metadata and address of a live value
    pub(crate) fn live(&self, slot: usize) -> Result<(M, NonNull<u8>), NotLive> {
        let slots = self.slots.borrow();
        let slot = slots.get(slot).ok_or(NotLive::Missing)?;

        match slot.state {
            SlotState::Live => Ok((slot.meta, slot.ptr())),
            SlotState::Initializing => Err(NotLive::Initializing),
            SlotState::Abandoned => Err(NotLive::Abandoned),
            SlotState::Removed => Err(NotLive::Removed),
        }
    }

    /// drops the live values whose metadata match `predicate`, in reverse reservation
    /// order, and returns their memory to the pool. Their slot numbers stay taken, so
    /// handles to them become invalid. Returns how many values were removed.
    pub(crate) fn remove_where(&mut self, mut predicate: impl FnMut(&M) -> bool) -> usize {
        let (pool, table) = (self.pool, self.table);
        let slots = self.slots.get_mut();
        let mut removed = 0;

        for slot in slots.iter_mut().rev() {
            if slot.state != SlotState::Live || !predicate(&slot.meta) {
                continue;
            }

            // mark first, so an unwinding destructor can't lead to a second drop
            slot.state = SlotState::Removed;
            removed += 1;

            // SAFETY: live slots hold an initialized value matching `drop_fn`
            unsafe { (slot.drop_fn)(slot.ptr().as_ptr()) };
            Self::release_memory(pool, table, slot);
        }

        removed
    }

    pub(crate) fn position(&self, mut predicate: impl FnMut(&M) -> bool) -> Option<usize> {
        self.slots
            .borrow()
            .iter()
            .position(|slot| predicate(&slot.meta))
    }

    /// the metadata of an existing slot, whatever its state
    pub(crate) fn meta(&self, slot: usize) -> Option<M> {
        self.slots.borrow().get(slot).map(|slot| slot.meta)
    }

    /// changes the metadata of an existing slot. `None` if the slot doesn't exist
    pub(crate) fn update_meta<R>(&self, slot: usize, f: impl FnOnce(&mut M) -> R) -> Option<R> {
        self.slots
            .borrow_mut()
            .get_mut(slot)
            .map(|slot| f(&mut slot.meta))
    }

    pub(crate) fn for_each_meta_mut(&self, mut f: impl FnMut(&mut M)) {
        for slot in self.slots.borrow_mut().iter_mut() {
            f(&mut slot.meta);
        }
    }

    pub(crate) fn is_borrowed(&self, slot: usize) -> bool {
        self.slots.borrow()[slot].borrow != BorrowState::Free
    }

    pub(crate) fn acquire(&self, slot: usize, kind: BorrowKind) -> Result<(), BorrowConflict> {
        let mut slots = self.slots.borrow_mut();
        let slot = &mut slots[slot];

        slot.borrow = slot.borrow.acquire(kind).ok_or(BorrowConflict)?;

        Ok(())
    }

    pub(crate) fn release(&self, slot: usize, kind: BorrowKind) {
        let mut slots = self.slots.borrow_mut();
        let slot = &mut slots[slot];

        slot.borrow = slot.borrow.release(kind);
    }

    /// drops every live value and empties the table, keeping the slot list's capacity
    pub(crate) fn clear(&mut self) {
        self.drop_values();
        self.slots.get_mut().clear();
    }
}

impl<M> SlotTable<M> {
    /// drops every live value and returns each slot's memory to the pool
    fn drop_values(&mut self) {
        let (pool, table) = (self.pool, self.table);
        let slots = self.slots.get_mut();

        for slot in slots.iter_mut().rev() {
            if slot.state == SlotState::Live {
                // mark first, so an unwinding destructor can't lead to a second drop
                slot.state = SlotState::Abandoned;

                // SAFETY: live slots hold an initialized value matching `drop_fn`
                unsafe { (slot.drop_fn)(slot.ptr().as_ptr()) };
            }

            Self::release_memory(pool, table, slot);
        }
    }
}

impl<M> Drop for SlotTable<M> {
    fn drop(&mut self) {
        self.drop_values();
    }
}
