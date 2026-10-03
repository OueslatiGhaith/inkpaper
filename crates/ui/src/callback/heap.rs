use core::{
    alloc::Layout,
    any::TypeId,
    cell::{Cell, RefCell},
    ptr::NonNull,
};

use alloc::{
    alloc::{alloc, dealloc},
    vec::Vec,
};

use crate::{
    CallbackAllocError, CanvasPainter, EntityId, Listener, ListenerInvokeError, PaintCx, Rect,
    callback::{
        CallbackId, CallbackStore,
        arena::{CallbackKind, CanvasInvokeError, RawCallbackReservation},
    },
    entity::EntityStore,
    global::GlobalStore,
};

#[cfg(test)]
#[path = "heap_tests.rs"]
mod tests;

struct Allocation {
    ptr: NonNull<u8>,
    layout: Layout,
}

impl Allocation {
    fn new(layout: Layout) -> Result<Self, CallbackAllocError> {
        let layout = Layout::from_size_align(layout.size().max(1), layout.align())
            .map_err(|_| CallbackAllocError::StorageFull)?;

        // SAFETY: the validated layout is nonzero, including for ZST closures.
        let ptr =
            NonNull::new(unsafe { alloc(layout) }).ok_or(CallbackAllocError::AllocationFailed)?;

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
    target: EntityId,
    generation: u32,
    kind: CallbackKind,
    drop_fn: unsafe fn(*mut u8),
    initialized: bool,
}

impl Drop for Entry {
    fn drop(&mut self) {
        if self.initialized {
            // SAFETY: commit follows initialization of the registered closure.
            // The Allocation field frees storage even if dropping captures unwinds.
            unsafe { (self.drop_fn)(self.allocation.ptr.as_ptr()) };
        }
    }
}

/// heap callback storage. The first allocation reserves `INITIAL_SLOTS` callbacks.
///
/// closure allocations never move. Invocation copies metadata before calling user code,
/// so callbacks may grow the slot table. Reset requires exclusive access and therefore
/// cannot free a closure while an invocation borrows this arena
pub struct HeapCallbackArena<const INITIAL_SLOTS: usize = 0> {
    entries: RefCell<Vec<Entry>>,
    generation: u32,
}

impl<const INITIAL_SLOTS: usize> Default for HeapCallbackArena<INITIAL_SLOTS> {
    fn default() -> Self {
        Self {
            entries: RefCell::new(Vec::new()),
            generation: 0,
        }
    }
}

impl<const INITIAL_SLOTS: usize> HeapCallbackArena<INITIAL_SLOTS> {
    fn lookup(&self, id: CallbackId) -> Option<(NonNull<u8>, EntityId, CallbackKind)> {
        if id.generation() != self.generation {
            return None;
        }

        let entries = self.entries.borrow();
        let entry = entries.get(usize::from(id.slot()))?;

        if !entry.initialized || entry.generation != id.generation() {
            return None;
        }

        Some((entry.allocation.ptr, entry.target, entry.kind))
    }

    pub(crate) fn invoke_listener<E: 'static>(
        &self,
        listener: Listener<E>,
        event: &E,
        entities: &dyn EntityStore,
        globals: &dyn GlobalStore,
        notified: &Cell<bool>,
    ) -> Result<(), ListenerInvokeError> {
        let (ptr, target, kind) = self
            .lookup(listener.id)
            .ok_or(ListenerInvokeError::InvalidListener)?;

        let CallbackKind::Listener {
            event_type,
            invoke_fn,
        } = kind
        else {
            return Err(ListenerInvokeError::InvalidListener);
        };

        if event_type != TypeId::of::<E>() {
            return Err(ListenerInvokeError::EventTypeMismatch);
        }

        // SAFETY: lookup validated a live closure and its generation; the event type
        // matches its trampoline. The &self borrow prevents reset during invocation.
        unsafe {
            invoke_fn(
                ptr.as_ptr(),
                target,
                event as *const E as *const u8,
                entities,
                globals,
                self,
                notified,
            )
        }
    }

    pub(crate) fn reset(&mut self) {
        // invalidate handles before dropping captures, including if a drop unwinds.
        self.generation = self.generation.wrapping_add(1);

        while let Some(entry) = self.entries.get_mut().pop() {
            drop(entry);
        }
    }
}

// SAFETY: reservations own correctly aligned storage. Only committed closures are
// callable. Both invocation paths validate generation/kind and use the registered
// trampoline, which enforces the target entity's borrow rules.
unsafe impl<const INITIAL_SLOTS: usize> CallbackStore for HeapCallbackArena<INITIAL_SLOTS> {
    fn reserve(
        &self,
        layout: Layout,
        target: EntityId,
        kind: CallbackKind,
        drop_fn: unsafe fn(*mut u8),
    ) -> Result<RawCallbackReservation, CallbackAllocError> {
        let mut entries = self.entries.borrow_mut();
        let slot = u16::try_from(entries.len()).map_err(|_| CallbackAllocError::SlotsFull)?;

        if entries.len() == entries.capacity() {
            let additional = if entries.is_empty() {
                INITIAL_SLOTS.max(1)
            } else {
                1
            };

            entries
                .try_reserve(additional)
                .map_err(|_| CallbackAllocError::AllocationFailed)?;
        }

        let allocation = Allocation::new(layout)?;
        let ptr = allocation.ptr;

        entries.push(Entry {
            allocation,
            target,
            generation: self.generation,
            kind,
            drop_fn,
            initialized: false,
        });

        Ok(RawCallbackReservation {
            id: CallbackId::new(slot, self.generation),
            ptr,
        })
    }

    unsafe fn commit(&self, callback: CallbackId) {
        let mut entries = self.entries.borrow_mut();
        let entry = &mut entries[usize::from(callback.slot())];

        debug_assert_eq!(callback.generation(), self.generation);
        debug_assert_eq!(entry.generation, callback.generation());
        debug_assert!(!entry.initialized);

        entry.initialized = true;
    }

    fn invoke_canvas(
        &self,
        callback: CallbackId,
        paint: &mut PaintCx<'_>,
        entities: &dyn EntityStore,
    ) -> Result<(), CanvasInvokeError> {
        let (ptr, target, kind) = self
            .lookup(callback)
            .ok_or(CanvasInvokeError::InvalidCallback)?;

        let CallbackKind::Canvas { invoke_fn } = kind else {
            return Err(CanvasInvokeError::CallbackKindMismatch);
        };

        // SAFETY: lookup validated the live closure. Its canvas trampoline acquires
        // a shared borrow of the target. Reset cannot run during this &self borrow.
        unsafe { invoke_fn(ptr.as_ptr(), target, paint, entities) }
    }
}

impl<const INITIAL_SLOTS: usize> crate::storage::CallbackStorage
    for HeapCallbackArena<INITIAL_SLOTS>
{
    fn reset(&mut self) {
        HeapCallbackArena::reset(self);
    }

    fn invoke_listener<E>(
        &self,
        listener: Listener<E>,
        event: &E,
        entities: &dyn EntityStore,
        globals: &dyn GlobalStore,
        notified: &Cell<bool>,
    ) -> Result<(), ListenerInvokeError>
    where
        E: 'static,
    {
        HeapCallbackArena::invoke_listener(self, listener, event, entities, globals, notified)
    }
}

impl<const INITIAL_SLOTS: usize> Drop for HeapCallbackArena<INITIAL_SLOTS> {
    fn drop(&mut self) {
        self.reset();
    }
}
