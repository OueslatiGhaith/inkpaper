use core::{alloc::Layout, any::TypeId, cell::Cell, ptr::NonNull};

use alloc::{
    alloc::{alloc, dealloc},
    vec::Vec,
};

use crate::{
    Global, GlobalAccessError, GlobalBorrowKind, GlobalSetError, GlobalStore,
    global::{GlobalBorrowState, drop_global},
};

struct Allocation {
    ptr: NonNull<u8>,
    layout: Layout,
}

impl Allocation {
    fn new<G>() -> Result<Self, GlobalSetError> {
        let layout = Layout::new::<G>();
        let layout = Layout::from_size_align(layout.size().max(1), layout.align())
            .map_err(|_| GlobalSetError::StorageFull)?;

        // SAFETY: the layout is valid and nonzero, including for ZSTs.
        let ptr = NonNull::new(unsafe { alloc(layout) }).ok_or(GlobalSetError::AllocationFailed)?;

        Ok(Self { ptr, layout })
    }
}

impl Drop for Allocation {
    fn drop(&mut self) {
        // SAFETY: this owner holds the original allocation and layout.
        unsafe { dealloc(self.ptr.as_ptr(), self.layout) };
    }
}

struct Entry {
    allocation: Allocation,
    type_id: TypeId,
    borrow: Cell<GlobalBorrowState>,
    drop_fn: unsafe fn(*mut u8),
}

impl Drop for Entry {
    fn drop(&mut self) {
        // SAFETY: entries own initialized globals of the registered type.
        // The Allocation field also frees storage when a destructor unwinds.
        unsafe { (self.drop_fn)(self.allocation.ptr.as_ptr()) };
    }
}

/// aach global has a stable allocation. SLOTS is a lazy reservation hint. BYTES does
/// not limit heap storage. Construction does not allocate.
pub(crate) struct GlobalArena<const BYTES: usize, const SLOTS: usize> {
    entries: Vec<Entry>,
    bytes: usize,
}

impl<const BYTES: usize, const SLOTS: usize> Default for GlobalArena<BYTES, SLOTS> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            bytes: 0,
        }
    }
}

impl<const BYTES: usize, const SLOTS: usize> GlobalArena<BYTES, SLOTS> {
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(crate) fn used_bytes(&self) -> usize {
        self.bytes
    }

    pub(crate) const fn capacity(&self) -> usize {
        self.entries.capacity()
    }

    pub(crate) const fn byte_capacity(&self) -> usize {
        self.bytes
    }

    pub(crate) fn contains<G: Global>(&self) -> bool {
        self.index_of(TypeId::of::<G>()).is_some()
    }

    pub(crate) fn set<G: Global>(&mut self, value: G) -> Result<(), GlobalSetError> {
        let type_id = TypeId::of::<G>();
        if let Some(slot) = self.index_of(type_id) {
            let entry = &self.entries[slot];
            if entry.borrow.get() != GlobalBorrowState::Free {
                return Err(GlobalSetError::BorrowConflict);
            }

            // SAFETY: TypeId matches G and no global guard borrows this value.
            // Install the replacement before running the old value's destructor.
            let old = unsafe { entry.allocation.ptr.cast::<G>().as_ptr().replace(value) };
            drop(old);
            return Ok(());
        }

        let bytes = self
            .bytes
            .checked_add(core::mem::size_of::<G>().max(1))
            .ok_or(GlobalSetError::StorageFull)?;

        if self.entries.len() == self.entries.capacity() {
            let additional = if self.entries.is_empty() {
                SLOTS.max(1)
            } else {
                1
            };
            self.entries
                .try_reserve(additional)
                .map_err(|_| GlobalSetError::AllocationFailed)?;
        }

        let allocation = Allocation::new::<G>()?;
        // SAFETY: storage has G's size and alignment and is exclusively owned.
        unsafe { allocation.ptr.cast::<G>().as_ptr().write(value) };
        self.entries.push(Entry {
            allocation,
            type_id,
            borrow: Cell::new(GlobalBorrowState::Free),
            drop_fn: drop_global::<G>,
        });
        self.bytes = bytes;
        Ok(())
    }

    fn index_of(&self, type_id: TypeId) -> Option<usize> {
        self.entries
            .iter()
            .position(|entry| entry.type_id == type_id)
    }
}

impl<const BYTES: usize, const SLOTS: usize> GlobalStore for GlobalArena<BYTES, SLOTS> {
    fn acquire(
        &self,
        type_id: TypeId,
        kind: GlobalBorrowKind,
    ) -> Result<(usize, NonNull<u8>), GlobalAccessError> {
        let slot = self.index_of(type_id).ok_or(GlobalAccessError::NotFound)?;
        let entry = &self.entries[slot];
        let next = match (kind, entry.borrow.get()) {
            (GlobalBorrowKind::Shared, GlobalBorrowState::Free) => GlobalBorrowState::Shared(1),
            (GlobalBorrowKind::Shared, GlobalBorrowState::Shared(count)) if count < u16::MAX => {
                GlobalBorrowState::Shared(count + 1)
            }
            (GlobalBorrowKind::Exclusive, GlobalBorrowState::Free) => GlobalBorrowState::Exclusive,
            _ => return Err(GlobalAccessError::BorrowConflict),
        };
        entry.borrow.set(next);
        Ok((slot, entry.allocation.ptr))
    }

    fn release(&self, slot: usize, kind: GlobalBorrowKind) {
        let state = &self.entries[slot].borrow;
        match (kind, state.get()) {
            (GlobalBorrowKind::Exclusive, GlobalBorrowState::Exclusive)
            | (GlobalBorrowKind::Shared, GlobalBorrowState::Shared(1)) => {
                state.set(GlobalBorrowState::Free);
            }
            (GlobalBorrowKind::Shared, GlobalBorrowState::Shared(count)) if count > 1 => {
                state.set(GlobalBorrowState::Shared(count - 1));
            }
            _ => debug_assert!(false, "released global without matching borrow"),
        }
    }
}

impl<const BYTES: usize, const SLOTS: usize> Drop for GlobalArena<BYTES, SLOTS> {
    fn drop(&mut self) {
        while let Some(entry) = self.entries.pop() {
            drop(entry);
        }
    }
}
