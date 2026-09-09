use core::{alloc::Layout, any::TypeId, cell::RefCell, ptr::NonNull};

use alloc::{
    alloc::{alloc, dealloc},
    vec::Vec,
};

use crate::{
    Entity, EntityAccessError, EntityAllocError, EntityBorrowKind, EntityId,
    entity::{BorrowState, EntityStore, RawEntityBorrow, RawEntityReservation, drop_value},
};

#[cfg(test)]
#[path = "allocated_tests.rs"]
mod tests;

struct Allocation {
    ptr: NonNull<u8>,
    layout: Layout,
}

impl Allocation {
    fn new(layout: Layout) -> Result<Self, EntityAllocError> {
        // give ZSTs distinct, correctly aligned storage as in the fixed arena
        let layout = Layout::from_size_align(layout.size().max(1), layout.align())
            .map_err(|_| EntityAllocError::StorageFull)?;

        // SAFETY: the layout has nonzero size and was validated above
        let ptr =
            NonNull::new(unsafe { alloc(layout) }).ok_or(EntityAllocError::AllocationFailed)?;

        Ok(Self { ptr, layout })
    }
}

impl Drop for Allocation {
    fn drop(&mut self) {
        // SAFETY: this allocation uniquely owns the pointer and its original layout
        unsafe { dealloc(self.ptr.as_ptr(), self.layout) };
    }
}

struct Entry {
    allocation: Option<Allocation>,
    type_id: TypeId,
    drop_fn: unsafe fn(*mut u8),
    initialized: bool,
    borrow: BorrowState,
}

impl Drop for Entry {
    fn drop(&mut self) {
        if self.initialized {
            let allocation = self.allocation.as_ref().expect("live entity has storage");
            // SAFETY: commit follows initialization of the registered type.
            // This entry uniquely owns the value. The allocation field frees storage
            // afterwards, including when the value's destructor unwinds
            unsafe { (self.drop_fn)(allocation.ptr.as_ptr()) };
        }
    }
}

/// values have independend allocations. Only metadata moves when the slot table grows.
/// No metadata reference or RefCell guard escapes an EntityStore call.
/// RawEntityBorrow retains a value pointer and releases its borrow by slot id.
/// Abandonned slots remain tombstones, so escaped construction handles stay invalid
pub(crate) struct EntityArena<const BYTES: usize, const SLOTS: usize> {
    entries: RefCell<Vec<Entry>>,
}

impl<const BYTES: usize, const SLOTS: usize> Default for EntityArena<BYTES, SLOTS> {
    fn default() -> Self {
        Self {
            entries: RefCell::new(Vec::new()),
        }
    }
}

impl<const BYTES: usize, const SLOTS: usize> EntityArena<BYTES, SLOTS> {
    pub fn len(&self) -> usize {
        self.entries.borrow().len()
    }

    pub fn used_bytes(&self) -> usize {
        self.entries
            .borrow()
            .iter()
            .filter_map(|entry| entry.allocation.as_ref())
            .map(|allocation| allocation.layout.size())
            .sum()
    }

    pub fn insert<T: 'static>(&self, value: T) -> Result<Entity<T>, EntityAllocError> {
        let reservation = self.reserve(Layout::new::<T>(), TypeId::of::<T>(), drop_value::<T>)?;

        // SAFETY: reserve provided suitably sized/aligned storage for T
        unsafe {
            reservation.ptr.cast::<T>().as_ptr().write(value);
            self.commit(reservation.id);
        }

        Ok(Entity::from_id(reservation.id))
    }

    pub fn read<T: 'static, R>(
        &self,
        entity: Entity<T>,
        f: impl FnOnce(&T) -> R,
    ) -> Result<R, EntityAccessError> {
        let borrow = RawEntityBorrow::acquire(
            self,
            entity.entity_id(),
            TypeId::of::<T>(),
            EntityBorrowKind::Shared,
        )?;

        // SAFETY: the guard validated T and holds a shared borrow until f returns
        Ok(f(unsafe { &*borrow.ptr().cast::<T>().as_ptr() }))
    }

    pub fn update<T: 'static, R>(
        &self,
        entity: Entity<T>,
        f: impl FnOnce(&mut T) -> R,
    ) -> Result<R, EntityAccessError> {
        let borrow = RawEntityBorrow::acquire(
            self,
            entity.entity_id(),
            TypeId::of::<T>(),
            EntityBorrowKind::Exclusive,
        )?;

        // SAFETY: the guard validated T and holds an exclusive borrow until f returns.
        Ok(f(unsafe { &mut *borrow.ptr().cast::<T>().as_ptr() }))
    }
}

// SAFETY: values never move, type/state checks precede access, and every returned pointer
// has a tracked borrow. Reservations are initialized only through commit
unsafe impl<const BYTES: usize, const SLOTS: usize> EntityStore for EntityArena<BYTES, SLOTS> {
    fn reserve(
        &self,
        layout: Layout,
        type_id: TypeId,
        drop_fn: unsafe fn(*mut u8),
    ) -> Result<RawEntityReservation, EntityAllocError> {
        let mut entries = self.entries.borrow_mut();
        let slot = u16::try_from(entries.len()).map_err(|_| EntityAllocError::SlotsFull)?;

        if entries.len() == entries.capacity() {
            let additional = if entries.is_empty() { SLOTS.max(1) } else { 1 };

            entries
                .try_reserve(additional)
                .map_err(|_| EntityAllocError::AllocationFailed)?;
        }

        let allocation = Allocation::new(layout)?;
        let ptr = allocation.ptr;

        entries.push(Entry {
            allocation: Some(allocation),
            type_id,
            drop_fn,
            initialized: false,
            borrow: BorrowState::Free,
        });

        Ok(RawEntityReservation {
            id: EntityId::new(slot, 0),
            ptr,
        })
    }

    unsafe fn commit(&self, entity: EntityId) {
        let mut entries = self.entries.borrow_mut();
        let entry = &mut entries[usize::from(entity.slot())];

        debug_assert_eq!(entity.generation(), 0);
        debug_assert!(!entry.initialized && entry.allocation.is_some());

        entry.initialized = true;
    }

    fn abandon(&self, entity: EntityId) {
        let mut entries = self.entries.borrow_mut();

        let Some(entry) = entries.get_mut(usize::from(entity.slot())) else {
            return;
        };

        if entity.generation() == 0 && !entry.initialized {
            entry.allocation.take();
        }
    }

    fn borrow(
        &self,
        entity: EntityId,
        type_id: TypeId,
        kind: EntityBorrowKind,
    ) -> Result<NonNull<u8>, EntityAccessError> {
        if entity.generation() != 0 {
            return Err(EntityAccessError::InvalidEntity);
        }

        let mut entries = self.entries.borrow_mut();
        let entry = entries
            .get_mut(usize::from(entity.slot()))
            .ok_or(EntityAccessError::InvalidEntity)?;

        let allocation = entry
            .allocation
            .as_ref()
            .ok_or(EntityAccessError::InvalidEntity)?;

        if !entry.initialized {
            return Err(EntityAccessError::NotReady);
        }
        if entry.type_id != type_id {
            return Err(EntityAccessError::TypeMismatch);
        }

        entry.borrow = match (kind, entry.borrow) {
            (EntityBorrowKind::Shared, BorrowState::Free) => BorrowState::Shared(1),
            (EntityBorrowKind::Shared, BorrowState::Shared(count)) if count < u16::MAX => {
                BorrowState::Shared(count + 1)
            }
            (EntityBorrowKind::Exclusive, BorrowState::Free) => BorrowState::Exclusive,
            _ => return Err(EntityAccessError::BorrowConflict),
        };

        Ok(allocation.ptr)
    }

    fn release(&self, entity: EntityId, kind: EntityBorrowKind) {
        let mut entries = self.entries.borrow_mut();
        let entry = &mut entries[usize::from(entity.slot())];

        entry.borrow = match (kind, entry.borrow) {
            (EntityBorrowKind::Exclusive, BorrowState::Exclusive) => BorrowState::Free,
            (EntityBorrowKind::Shared, BorrowState::Shared(1)) => BorrowState::Free,
            (EntityBorrowKind::Shared, BorrowState::Shared(count)) if count > 1 => {
                BorrowState::Shared(count - 1)
            }
            _ => {
                debug_assert!(false, "released entity without matching borrow");
                entry.borrow
            }
        };
    }
}

impl<const BYTES: usize, const SLOTS: usize> Drop for EntityArena<BYTES, SLOTS> {
    fn drop(&mut self) {
        // match the fixed arena's reverse reservation order.
        while let Some(entry) = self.entries.get_mut().pop() {
            drop(entry);
        }
    }
}
