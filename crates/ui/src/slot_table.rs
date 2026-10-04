//! the slot table shared by the entity, callback and global arenas.
//!
//! a [`SlotTable`] stores type-erased values. Each slot records where its value lives,
//! arena-specific metadata, the value's destructor, its lifecycle state and its borrow
//! state. [`SlotStorage`] picks where values and slots live: [`FixedSlots`] places values
//! in an inline byte buffer, [`HeapSlots`] gives each value its own allocation.
//!
//! SAFETY INVARIANTS:
//!
//! 1. a place is never reused while its slot exists, so values never move and never
//!    overlap. Growing the slot list moves slots, not values.
//! 2. only `Live` slots hold an initialized value. Each is dropped exactly once: when it
//!    is removed, or when the table is cleared or dropped, in reverse reservation order.
//! 3. no `RefCell` guard on the slot list is held while user code runs, so callbacks may
//!    reserve new slots.
//! 4. no reference to a fixed byte buffer as a whole is created while references to
//!    stored values may exist.
//! 5. borrow states only count borrows. Arenas pair each `acquire` with one `release`
//!    and convert pointers into references only while the matching borrow is held.

use core::{alloc::Layout, cell::RefCell, ptr::NonNull};

use crate::storage::VecStorage;

/// where a slot table keeps its values and slots
pub trait SlotStorage {
    type Memory: ValueMemory;
    type Slots<T>: VecStorage<T>;
}

/// at most `SLOTS` values in an inline buffer of `BYTES` bytes. Never allocates.
pub struct FixedSlots<const BYTES: usize, const SLOTS: usize>;

impl<const BYTES: usize, const SLOTS: usize> SlotStorage for FixedSlots<BYTES, SLOTS> {
    type Memory = FixedMemory<BYTES>;
    type Slots<T> = heapless::Vec<T, SLOTS>;
}

/// one allocation per value. The first slot allocation reserves `INITIAL_SLOTS` slots.
#[cfg(feature = "alloc")]
pub struct HeapSlots<const INITIAL_SLOTS: usize>;

#[cfg(feature = "alloc")]
impl<const INITIAL_SLOTS: usize> SlotStorage for HeapSlots<INITIAL_SLOTS> {
    type Memory = HeapMemory;
    type Slots<T> = crate::storage::HeapVec<T, INITIAL_SLOTS>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReserveError {
    SlotsFull,
    StorageFull,
    UnsupportedAlignment { requested: usize, supported: usize },
    AllocationFailed,
}

/// memory for the values of a slot table
pub trait ValueMemory: Default {
    /// where one value lives. Dropping it releases the memory, not the value.
    type Place;

    /// reserves memory for one value. Zero-sized values still get distinct places.
    fn place(&self, layout: Layout) -> Result<Self::Place, ReserveError>;

    /// the value's address. Only called for places that still hold memory.
    fn ptr(&self, place: &Self::Place) -> NonNull<u8>;

    /// releases an abandoned or removed place's memory early, if the memory can
    fn free(&self, place: &mut Self::Place);

    /// called once all places have been dropped
    fn reset(&mut self);

    fn used_bytes(&self) -> usize;

    fn byte_capacity(&self) -> usize;
}

const FIXED_ALIGNMENT: usize = 16;

#[repr(C, align(16))]
struct Bytes<const N: usize> {
    bytes: [core::mem::MaybeUninit<u8>; N],
}

/// bump allocation in an inline buffer of `BYTES` bytes, aligned to 16
pub struct FixedMemory<const BYTES: usize> {
    storage: core::cell::UnsafeCell<Bytes<BYTES>>,
    cursor: core::cell::Cell<usize>,
}

impl<const BYTES: usize> Default for FixedMemory<BYTES> {
    fn default() -> Self {
        Self {
            storage: core::cell::UnsafeCell::new(Bytes {
                bytes: [core::mem::MaybeUninit::uninit(); BYTES],
            }),
            cursor: core::cell::Cell::new(0),
        }
    }
}

impl<const BYTES: usize> FixedMemory<BYTES> {
    fn base(&self) -> *mut u8 {
        let storage = self.storage.get();

        // SAFETY: creates a raw pointer to the buffer without referencing it as a whole
        unsafe { core::ptr::addr_of_mut!((*storage).bytes).cast::<u8>() }
    }
}

impl<const BYTES: usize> ValueMemory for FixedMemory<BYTES> {
    /// offset into the buffer
    type Place = usize;

    fn place(&self, layout: Layout) -> Result<usize, ReserveError> {
        let alignment = layout.align();
        if alignment > FIXED_ALIGNMENT {
            return Err(ReserveError::UnsupportedAlignment {
                requested: alignment,
                supported: FIXED_ALIGNMENT,
            });
        }

        let offset = align_up(self.cursor.get(), alignment).ok_or(ReserveError::StorageFull)?;
        let end = offset
            .checked_add(layout.size().max(1))
            .ok_or(ReserveError::StorageFull)?;
        if end > BYTES {
            return Err(ReserveError::StorageFull);
        }

        self.cursor.set(end);

        Ok(offset)
    }

    fn ptr(&self, offset: &usize) -> NonNull<u8> {
        // SAFETY: `place` checked that the offset lies inside the buffer
        unsafe { NonNull::new_unchecked(self.base().add(*offset)) }
    }

    fn free(&self, _: &mut usize) {}

    fn reset(&mut self) {
        self.cursor.set(0);
    }

    fn used_bytes(&self) -> usize {
        self.cursor.get()
    }

    fn byte_capacity(&self) -> usize {
        BYTES
    }
}

fn align_up(value: usize, alignment: usize) -> Option<usize> {
    debug_assert!(alignment.is_power_of_two());

    let mask = alignment - 1;
    value.checked_add(mask).map(|v| v & !mask)
}

/// one allocation per value, so any alignment is supported
#[cfg(feature = "alloc")]
#[derive(Default)]
pub struct HeapMemory {
    used: core::cell::Cell<usize>,
}

/// an owned allocation, or `None` once freed
#[cfg(feature = "alloc")]
pub struct HeapPlace(Option<(NonNull<u8>, Layout)>);

#[cfg(feature = "alloc")]
impl Drop for HeapPlace {
    fn drop(&mut self) {
        if let Some((ptr, layout)) = self.0.take() {
            // SAFETY: this place owns the allocation and its original layout
            unsafe { alloc::alloc::dealloc(ptr.as_ptr(), layout) };
        }
    }
}

#[cfg(feature = "alloc")]
impl ValueMemory for HeapMemory {
    type Place = HeapPlace;

    fn place(&self, layout: Layout) -> Result<HeapPlace, ReserveError> {
        let layout = Layout::from_size_align(layout.size().max(1), layout.align())
            .map_err(|_| ReserveError::StorageFull)?;
        let used = self
            .used
            .get()
            .checked_add(layout.size())
            .ok_or(ReserveError::StorageFull)?;

        // SAFETY: the layout has a nonzero size
        let ptr = NonNull::new(unsafe { alloc::alloc::alloc(layout) })
            .ok_or(ReserveError::AllocationFailed)?;

        self.used.set(used);

        Ok(HeapPlace(Some((ptr, layout))))
    }

    fn ptr(&self, place: &HeapPlace) -> NonNull<u8> {
        place.0.expect("place holds memory").0
    }

    fn free(&self, place: &mut HeapPlace) {
        if let Some((_, layout)) = place.0 {
            self.used.set(self.used.get() - layout.size());
            drop(core::mem::replace(place, HeapPlace(None)));
        }
    }

    fn reset(&mut self) {
        self.used.set(0);
    }

    fn used_bytes(&self) -> usize {
        self.used.get()
    }

    /// heap memory has no fixed capacity, so this is the bytes in use
    fn byte_capacity(&self) -> usize {
        self.used.get()
    }
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

struct Slot<P, M> {
    place: P,
    meta: M,
    drop_fn: unsafe fn(*mut u8),
    state: SlotState,
    borrow: BorrowState,
}

type Place<S> = <<S as SlotStorage>::Memory as ValueMemory>::Place;
type Slots<S, M> = <S as SlotStorage>::Slots<Slot<Place<S>, M>>;

/// type-erased values with per-slot metadata `M`
pub(crate) struct SlotTable<S: SlotStorage, M> {
    memory: S::Memory,
    slots: RefCell<Slots<S, M>>,
}

impl<S: SlotStorage, M> Default for SlotTable<S, M> {
    fn default() -> Self {
        Self {
            memory: S::Memory::default(),
            slots: RefCell::new(Slots::<S, M>::default()),
        }
    }
}

impl<S: SlotStorage, M: Copy> SlotTable<S, M> {
    pub(crate) fn len(&self) -> usize {
        self.slots.borrow().len()
    }

    pub(crate) fn capacity(&self) -> usize {
        self.slots.borrow().capacity()
    }

    pub(crate) fn used_bytes(&self) -> usize {
        self.memory.used_bytes()
    }

    pub(crate) fn byte_capacity(&self) -> usize {
        self.memory.byte_capacity()
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

        // reserve the slot first, so a full slot list doesn't consume memory
        slots
            .try_reserve(1)
            .map_err(|error| error.or(ReserveError::SlotsFull, ReserveError::AllocationFailed))?;

        let place = self.memory.place(layout)?;
        let ptr = self.memory.ptr(&place);

        slots
            .try_push(Slot {
                place,
                meta,
                drop_fn,
                state: SlotState::Initializing,
                borrow: BorrowState::Free,
            })
            .unwrap_or_else(|_| unreachable!("slot was reserved"));

        Ok((slot, ptr))
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
            self.memory.free(&mut slot.place);
        }
    }

    /// the metadata and address of a live value
    pub(crate) fn live(&self, slot: usize) -> Result<(M, NonNull<u8>), NotLive> {
        let slots = self.slots.borrow();
        let slot = slots.get(slot).ok_or(NotLive::Missing)?;

        match slot.state {
            SlotState::Live => Ok((slot.meta, self.memory.ptr(&slot.place))),
            SlotState::Initializing => Err(NotLive::Initializing),
            SlotState::Abandoned => Err(NotLive::Abandoned),
            SlotState::Removed => Err(NotLive::Removed),
        }
    }

    /// drops the live values whose slot number and metadata match `predicate`, in
    /// reverse reservation order, and releases their memory if the memory can. Their
    /// slot numbers stay taken, so handles to them become invalid. Returns how many
    /// values were removed.
    pub(crate) fn remove_where(&mut self, mut predicate: impl FnMut(usize, &M) -> bool) -> usize {
        let slots = self.slots.get_mut();
        let mut removed = 0;

        for (index, slot) in slots.iter_mut().enumerate().rev() {
            if slot.state != SlotState::Live || !predicate(index, &slot.meta) {
                continue;
            }

            // mark first, so an unwinding destructor can't lead to a second drop
            slot.state = SlotState::Removed;
            removed += 1;

            // SAFETY: live slots hold an initialized value matching `drop_fn`
            unsafe { (slot.drop_fn)(self.memory.ptr(&slot.place).as_ptr()) };
            self.memory.free(&mut slot.place);
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
        self.memory.reset();
    }
}

impl<S: SlotStorage, M> SlotTable<S, M> {
    fn drop_values(&mut self) {
        let slots = self.slots.get_mut();

        for slot in slots.iter_mut().rev() {
            if slot.state != SlotState::Live {
                continue;
            }

            // mark first, so an unwinding destructor can't lead to a second drop
            slot.state = SlotState::Abandoned;

            // SAFETY: live slots hold an initialized value matching `drop_fn`
            unsafe { (slot.drop_fn)(self.memory.ptr(&slot.place).as_ptr()) };
        }
    }
}

impl<S: SlotStorage, M> Drop for SlotTable<S, M> {
    fn drop(&mut self) {
        self.drop_values();
    }
}
