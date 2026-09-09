use core::{
    alloc::Layout,
    any::TypeId,
    cell::{Cell, RefCell, UnsafeCell},
    mem::MaybeUninit,
    ptr::NonNull,
};

use heapless::Vec;

use crate::{
    CanvasPainter, Context, Entity, EntityAccessError, EntityId, Listener, Rect,
    callback::CallbackId,
    entity::{EntityBorrowKind, EntityStore, RawEntityBorrow, align_up},
    global::GlobalStore,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallbackAllocError {
    SlotsFull,
    StorageFull,
    UnsupportedAlignment {
        requested: usize,
        supported: usize,
    },
    #[cfg(feature = "alloc")]
    AllocationFailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListenerInvokeError {
    InvalidListener,
    EventTypeMismatch,
    Entity(EntityAccessError),
}

impl From<EntityAccessError> for ListenerInvokeError {
    fn from(value: EntityAccessError) -> Self {
        Self::Entity(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CanvasInvokeError {
    InvalidCallback,
    CallbackKindMismatch,
    Entity(EntityAccessError),
}

impl From<EntityAccessError> for CanvasInvokeError {
    fn from(value: EntityAccessError) -> Self {
        Self::Entity(value)
    }
}

type ListenerInvokeFn = unsafe fn(
    closure: *const u8,
    target: EntityId,
    event: *const u8,
    entities: &dyn EntityStore,
    globals: &dyn GlobalStore,
    callbacks: &dyn CallbackStore,
    notified: &Cell<bool>,
) -> Result<(), ListenerInvokeError>;

type CanvasInvokeFn = unsafe fn(
    closure: *const u8,
    target: EntityId,
    bounds: Rect,
    painter: &mut dyn CanvasPainter,
    entities: &dyn EntityStore,
) -> Result<(), CanvasInvokeError>;

#[derive(Clone, Copy)]
pub(crate) enum CallbackKind {
    Listener {
        event_type: TypeId,
        invoke_fn: ListenerInvokeFn,
    },
    Canvas {
        invoke_fn: CanvasInvokeFn,
    },
}

#[derive(Clone, Copy)]
struct CallbackMeta {
    offset: usize,
    target: EntityId,
    generation: u32,
    kind: CallbackKind,
    drop_fn: unsafe fn(*mut u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CallbackSlotState {
    Vacant,
    Initializing,
    Live,
    Abandonned,
}

pub(crate) struct RawCallbackReservation {
    pub(crate) id: CallbackId,
    pub(crate) ptr: NonNull<u8>,
}

/// # Safety
///
/// the reserved storage for `callback` must contain a valid initialized
/// callback of the type registered during `reserve`.
pub(crate) unsafe trait CallbackStore {
    fn reserve(
        &self,
        layout: Layout,
        target: EntityId,
        kind: CallbackKind,
        drop_fn: unsafe fn(*mut u8),
    ) -> Result<RawCallbackReservation, CallbackAllocError>;

    unsafe fn commit(&self, callback: CallbackId);

    fn invoke_canvas(
        &self,
        callback: CallbackId,
        bounds: Rect,
        painter: &mut dyn CanvasPainter,
        entities: &dyn EntityStore,
    ) -> Result<(), CanvasInvokeError>;
}

const CALLBACK_ARENA_ALIGNMENT: usize = 16;

#[repr(C, align(16))]
struct CallbackStorage<const N: usize> {
    bytes: [MaybeUninit<u8>; N],
}

impl<const N: usize> CallbackStorage<N> {
    const fn new() -> Self {
        Self {
            bytes: [MaybeUninit::uninit(); N],
        }
    }
}

/// SAFETY INVARIANTS:
///
/// 1. every Live [`CallbackMeta`] refers to one initialized callback F.
/// 2. callback storage remains fixed until reset.
/// 3. callback generation must match metadata generation before invocation.
/// 4. event [`TypeId`] is checked before casting event pointer to E.
/// 5. the callback trampoline used for a callback matches the concrete F, T, and E used when
///    that callback was registered.
/// 6. the callback target is exclusively borrowed through [`EntityStore`] for
///    the complete callback.
/// 7. only Live callbacks are dropped.
/// 8. each live callback is dropped exactly once.
pub(crate) struct CallbackArena<const BYTES: usize, const SLOTS: usize> {
    storage: UnsafeCell<CallbackStorage<BYTES>>,
    entries: RefCell<Vec<CallbackMeta, SLOTS>>,
    states: [Cell<CallbackSlotState>; SLOTS],
    cursor: Cell<usize>,
    generation: Cell<u32>,
}

impl<const BYTES: usize, const SLOTS: usize> Default for CallbackArena<BYTES, SLOTS> {
    fn default() -> Self {
        Self {
            storage: UnsafeCell::new(CallbackStorage::new()),
            entries: RefCell::new(Vec::new()),
            states: core::array::from_fn(|_| Cell::new(CallbackSlotState::Vacant)),
            cursor: Cell::new(0),
            generation: Cell::new(0),
        }
    }
}

impl<const BYTES: usize, const SLOTS: usize> CallbackArena<BYTES, SLOTS> {
    fn storage_ptr(&self) -> *mut u8 {
        let storage = self.storage.get();

        unsafe {
            core::ptr::addr_of_mut!((*storage).bytes)
                .cast::<MaybeUninit<u8>>()
                .cast::<u8>()
        }
    }

    pub fn invoke_listener<E>(
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
        let id = listener.id;
        let slot = id.slot() as usize;

        let meta = {
            let entries = self.entries.borrow();
            entries
                .get(slot)
                .copied()
                .ok_or(ListenerInvokeError::InvalidListener)?
        };

        if meta.generation != id.generation() {
            return Err(ListenerInvokeError::InvalidListener);
        }
        if self.states[slot].get() != CallbackSlotState::Live {
            return Err(ListenerInvokeError::InvalidListener);
        }

        let CallbackKind::Listener {
            event_type,
            invoke_fn,
        } = meta.kind
        else {
            return Err(ListenerInvokeError::InvalidListener);
        };

        if event_type != TypeId::of::<E>() {
            return Err(ListenerInvokeError::EventTypeMismatch);
        }

        let closure = unsafe { self.storage_ptr().add(meta.offset) };

        unsafe {
            invoke_fn(
                closure,
                meta.target,
                event as *const E as *const u8,
                entities,
                globals,
                self,
                notified,
            )
        }
    }

    fn drop_live_callbacks(&mut self) {
        let storage_ptr = {
            let storage = self.storage.get();
            unsafe {
                core::ptr::addr_of_mut!((*storage).bytes)
                    .cast::<MaybeUninit<u8>>()
                    .cast::<u8>()
            }
        };

        let entries = self.entries.get_mut();

        for (slot, meta) in entries.iter().enumerate().rev() {
            if self.states[slot].get() != CallbackSlotState::Live {
                continue;
            }

            let callback_ptr = unsafe { storage_ptr.add(meta.offset) };
            unsafe { (meta.drop_fn)(callback_ptr) };

            self.states[slot].set(CallbackSlotState::Abandonned);
        }
    }

    pub fn reset(&mut self) {
        self.drop_live_callbacks();

        self.entries.get_mut().clear();

        for state in &self.states {
            state.set(CallbackSlotState::Vacant);
        }

        self.cursor.set(0);
        self.generation.set(self.generation.get().wrapping_add(1));
    }

    fn invoke_canvas_callback(
        &self,
        callback: CallbackId,
        bounds: Rect,
        painter: &mut dyn CanvasPainter,
        entities: &dyn EntityStore,
    ) -> Result<(), CanvasInvokeError> {
        let slot = callback.slot() as usize;
        let meta = self
            .entries
            .borrow()
            .get(slot)
            .copied()
            .ok_or(CanvasInvokeError::InvalidCallback)?;

        if meta.generation != callback.generation() {
            return Err(CanvasInvokeError::InvalidCallback);
        }
        if self.states[slot].get() != CallbackSlotState::Live {
            return Err(CanvasInvokeError::InvalidCallback);
        }

        let CallbackKind::Canvas { invoke_fn } = meta.kind else {
            return Err(CanvasInvokeError::CallbackKindMismatch);
        };

        let closure = unsafe { self.storage_ptr().add(meta.offset) };

        unsafe { invoke_fn(closure, meta.target, bounds, painter, entities) }
    }
}

unsafe impl<const BYTES: usize, const SLOTS: usize> CallbackStore for CallbackArena<BYTES, SLOTS> {
    fn reserve(
        &self,
        layout: Layout,
        target: EntityId,
        kind: CallbackKind,
        drop_fn: unsafe fn(*mut u8),
    ) -> Result<RawCallbackReservation, CallbackAllocError> {
        let mut entries = self.entries.borrow_mut();
        let slot = entries.len();

        if slot >= SLOTS || slot > u16::MAX as usize {
            return Err(CallbackAllocError::SlotsFull);
        }

        let alignment = layout.align();
        if alignment > CALLBACK_ARENA_ALIGNMENT {
            return Err(CallbackAllocError::UnsupportedAlignment {
                requested: alignment,
                supported: CALLBACK_ARENA_ALIGNMENT,
            });
        }

        let offset =
            align_up(self.cursor.get(), alignment).ok_or(CallbackAllocError::StorageFull)?;

        // reserve at least one byte for ZST so separate entries still receive
        // distinct storage locations
        let allocation_size = layout.size().max(1);

        let end = offset
            .checked_add(allocation_size)
            .ok_or(CallbackAllocError::StorageFull)?;
        if end > BYTES {
            return Err(CallbackAllocError::StorageFull);
        }

        let generation = self.generation.get();
        let id = CallbackId::new(slot as u16, generation);

        let meta = CallbackMeta {
            offset,
            target,
            generation,
            kind,
            drop_fn,
        };

        entries
            .push(meta)
            .map_err(|_| CallbackAllocError::SlotsFull)?;

        self.cursor.set(end);
        self.states[slot].set(CallbackSlotState::Initializing);

        let ptr = unsafe { self.storage_ptr().add(offset) };
        let ptr = unsafe { NonNull::new_unchecked(ptr) };

        Ok(RawCallbackReservation { id, ptr })
    }

    unsafe fn commit(&self, callback: CallbackId) {
        let slot = callback.slot() as usize;
        self.states[slot].set(CallbackSlotState::Live);
    }

    fn invoke_canvas(
        &self,
        callback: CallbackId,
        bounds: Rect,
        painter: &mut dyn CanvasPainter,
        entities: &dyn EntityStore,
    ) -> Result<(), CanvasInvokeError> {
        self.invoke_canvas_callback(callback, bounds, painter, entities)
    }
}

impl<const BYTES: usize, const SLOTS: usize> Drop for CallbackArena<BYTES, SLOTS> {
    fn drop(&mut self) {
        self.drop_live_callbacks();
    }
}

unsafe fn drop_callback<F>(ptr: *mut u8) {
    unsafe { core::ptr::drop_in_place(ptr.cast::<F>()) };
}

unsafe fn invoke_listener_callback<T, E, F>(
    closure: *const u8,
    target: EntityId,
    event: *const u8,
    entities: &dyn EntityStore,
    globals: &dyn GlobalStore,
    callbacks: &dyn CallbackStore,
    notified: &Cell<bool>,
) -> Result<(), ListenerInvokeError>
where
    T: 'static,
    E: 'static,
    F: Fn(&mut T, &E, &mut Context<'_, T>) + 'static,
{
    let borrow = RawEntityBorrow::acquire(
        entities,
        target,
        TypeId::of::<T>(),
        EntityBorrowKind::Exclusive,
    )?;

    let state = unsafe { &mut *borrow.ptr().cast::<T>().as_ptr() };
    let event = unsafe { &*event.cast::<E>() };
    let callback = unsafe { &*closure.cast::<F>() };
    let entity = Entity::<T>::from_id(target);
    let mut cx = Context::from_parts(entity, entities, globals, callbacks, notified);

    callback(state, event, &mut cx);

    Ok(())
}

unsafe fn invoke_canvas_callback<T, F>(
    closure: *const u8,
    target: EntityId,
    bounds: Rect,
    painter: &mut dyn CanvasPainter,
    entities: &dyn EntityStore,
) -> Result<(), CanvasInvokeError>
where
    T: 'static,
    F: Fn(&T, Rect, &mut dyn CanvasPainter) + 'static,
{
    let borrow = RawEntityBorrow::acquire(
        entities,
        target,
        TypeId::of::<T>(),
        EntityBorrowKind::Shared,
    )?;

    let state = unsafe { &*borrow.ptr().cast::<T>().as_ptr() };
    let callback = unsafe { &*closure.cast::<F>() };

    callback(state, bounds, painter);

    Ok(())
}

pub(crate) fn register_listener<T, E, F>(
    store: &dyn CallbackStore,
    entity: Entity<T>,
    callback: F,
) -> Result<Listener<E>, CallbackAllocError>
where
    T: 'static,
    E: 'static,
    F: Fn(&mut T, &E, &mut Context<'_, T>) + 'static,
{
    let reservation = store.reserve(
        Layout::new::<F>(),
        entity.entity_id(),
        CallbackKind::Listener {
            event_type: TypeId::of::<E>(),
            invoke_fn: invoke_listener_callback::<T, E, F>,
        },
        drop_callback::<F>,
    )?;

    unsafe { reservation.ptr.cast::<F>().as_ptr().write(callback) };
    unsafe { store.commit(reservation.id) };

    Ok(Listener::from_id(reservation.id))
}

pub(crate) fn register_canvas_callback<T, F>(
    store: &dyn CallbackStore,
    entity: Entity<T>,
    callback: F,
) -> Result<CallbackId, CallbackAllocError>
where
    T: 'static,
    F: Fn(&T, Rect, &mut dyn CanvasPainter) + 'static,
{
    let reservation = store.reserve(
        Layout::new::<F>(),
        entity.entity_id(),
        CallbackKind::Canvas {
            invoke_fn: invoke_canvas_callback::<T, F>,
        },
        drop_callback::<F>,
    )?;

    unsafe { reservation.ptr.cast::<F>().as_ptr().write(callback) };
    unsafe { store.commit(reservation.id) };

    Ok(reservation.id)
}

#[cfg(test)]
mod tests {
    use core::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::callback::CallbackArena;
    use crate::*;

    struct Counter {
        value: i32,
    }

    impl Counter {
        fn increment(&mut self, _: &ActivateEvent, cx: &mut Context<Self>) {
            self.value += 1;
            cx.notify();
        }
    }

    #[test]
    fn invokes_method_listener() {
        let entities = EntityArena::<1024, 16>::default();
        let callbacks = CallbackArena::<1024, 16>::default();
        let globals = GlobalArena::<0, 0>::default();

        let root = entities.insert(Counter { value: 0 }).unwrap();

        let notified = Cell::new(false);

        let listener = {
            let mut cx = Context::from_parts(root, &entities, &globals, &callbacks, &notified);
            cx.listener(Counter::increment)
        };

        callbacks
            .invoke_listener(listener, &ActivateEvent, &entities, &globals, &notified)
            .unwrap();

        assert_eq!(entities.read(root, |counter| { counter.value }), Ok(1));
        assert!(notified.get());
    }

    #[test]
    fn invokes_capturing_listener() {
        let entities = EntityArena::<1024, 16>::default();
        let callbacks = CallbackArena::<1024, 16>::default();
        let globals = GlobalArena::<0, 0>::default();
        let counter = entities.insert(Counter { value: 0 }).unwrap();
        let notified = Cell::new(false);
        let amount = 5;

        let listener = {
            let mut cx = Context::from_parts(counter, &entities, &globals, &callbacks, &notified);

            cx.listener(move |counter: &mut Counter, _: &ActivateEvent, cx| {
                counter.value += amount;
                cx.notify();
            })
        };

        callbacks
            .invoke_listener(listener, &ActivateEvent, &entities, &globals, &notified)
            .unwrap();

        assert_eq!(entities.read(counter, |counter| counter.value), Ok(5));
        assert!(notified.get());
    }

    #[test]
    fn listener_rejects_reentrant_update_of_target_entity() {
        let entities = EntityArena::<1024, 16>::default();
        let callbacks = CallbackArena::<1024, 16>::default();
        let globals = GlobalArena::<0, 0>::default();
        let counter = entities.insert(Counter { value: 0 }).unwrap();
        let notified = Cell::new(false);

        let listener = {
            let mut cx = Context::from_parts(counter, &entities, &globals, &callbacks, &notified);

            cx.listener(move |_: &mut Counter, _: &ActivateEvent, cx| {
                let result = counter.update(cx, |_, _| {});

                assert!(matches!(result, Err(EntityAccessError::BorrowConflict)));
            })
        };

        callbacks
            .invoke_listener(listener, &ActivateEvent, &entities, &globals, &notified)
            .unwrap();
    }

    struct Settings {
        dirty: bool,
    }

    #[test]
    fn listener_can_update_another_entity() {
        let entities = EntityArena::<1024, 16>::default();
        let callbacks = CallbackArena::<1024, 16>::default();
        let globals = GlobalArena::<0, 0>::default();
        let counter = entities.insert(Counter { value: 0 }).unwrap();
        let settings = entities.insert(Settings { dirty: false }).unwrap();
        let notified = Cell::new(false);

        let listener = {
            let mut cx = Context::from_parts(counter, &entities, &globals, &callbacks, &notified);

            cx.listener(move |counter: &mut Counter, _: &ActivateEvent, cx| {
                counter.value += 1;

                settings
                    .update(cx, |settings, cx| {
                        settings.dirty = true;
                        cx.notify();
                    })
                    .unwrap();
            })
        };

        callbacks
            .invoke_listener(listener, &ActivateEvent, &entities, &globals, &notified)
            .unwrap();

        assert_eq!(entities.read(counter, |counter| counter.value), Ok(1));
        assert!(entities.read(settings, |settings| settings.dirty).unwrap());
        assert!(notified.get());
    }

    #[test]
    fn stale_listener_is_rejected_after_reset() {
        let entities = EntityArena::<1024, 16>::default();
        let mut callbacks = CallbackArena::<1024, 16>::default();
        let globals = GlobalArena::<0, 0>::default();
        let counter = entities.insert(Counter { value: 0 }).unwrap();
        let notified = Cell::new(false);

        let listener = {
            let mut cx = Context::from_parts(counter, &entities, &globals, &callbacks, &notified);

            cx.listener(Counter::increment)
        };

        callbacks.reset();

        let result =
            callbacks.invoke_listener(listener, &ActivateEvent, &entities, &globals, &notified);

        assert!(matches!(result, Err(ListenerInvokeError::InvalidListener)));
    }

    static DROPS: AtomicUsize = AtomicUsize::new(0);

    struct DroppableCapture;

    impl Drop for DroppableCapture {
        fn drop(&mut self) {
            DROPS.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn reset_drops_captured_listener_values() {
        DROPS.store(0, Ordering::SeqCst);

        let entities = EntityArena::<1024, 16>::default();
        let mut callbacks = CallbackArena::<1024, 16>::default();
        let globals = GlobalArena::<0, 0>::default();
        let counter = entities.insert(Counter { value: 0 }).unwrap();
        let notified = Cell::new(false);

        {
            let capture = DroppableCapture;

            let mut cx = Context::from_parts(counter, &entities, &globals, &callbacks, &notified);

            let _listener = cx.listener(move |_: &mut Counter, _: &ActivateEvent, _| {
                let _ = &capture;
            });
        }

        assert_eq!(DROPS.load(Ordering::SeqCst), 0);

        callbacks.reset();

        assert_eq!(DROPS.load(Ordering::SeqCst), 1);
    }

    #[test]
    #[cfg(not(feature = "alloc"))]
    fn listener_arena_reports_slot_exhaustion() {
        let entities = EntityArena::<1024, 16>::default();
        let callbacks = CallbackArena::<1024, 1>::default();
        let globals = GlobalArena::<0, 0>::default();
        let counter = entities.insert(Counter { value: 0 }).unwrap();
        let notified = Cell::new(false);

        let mut cx = Context::from_parts(counter, &entities, &globals, &callbacks, &notified);

        cx.try_listener::<ActivateEvent, _>(|_: &mut Counter, _, _| {})
            .unwrap();

        let result = cx.try_listener::<ActivateEvent, _>(|_: &mut Counter, _, _| {});

        assert!(matches!(result, Err(CallbackAllocError::SlotsFull)));
    }

    #[test]
    #[cfg(not(feature = "alloc"))]
    fn listener_arena_reports_storage_exhaustion() {
        let entities = EntityArena::<1024, 16>::default();
        let callbacks = CallbackArena::<4, 16>::default();
        let globals = GlobalArena::<0, 0>::default();
        let counter = entities.insert(Counter { value: 0 }).unwrap();
        let notified = Cell::new(false);
        let capture = [0u8; 32];

        let mut cx = Context::from_parts(counter, &entities, &globals, &callbacks, &notified);

        let result = cx.try_listener::<ActivateEvent, _>(move |_: &mut Counter, _, _| {
            let _ = &capture;
        });

        assert!(matches!(result, Err(CallbackAllocError::StorageFull)));
    }

    #[test]
    fn click_listener_can_be_attached_to_stateful_div() {
        let entities = EntityArena::<1024, 16>::default();
        let callbacks = CallbackArena::<1024, 16>::default();
        let globals = GlobalArena::<0, 0>::default();
        let counter = entities.insert(Counter { value: 0 }).unwrap();
        let notified = Cell::new(false);

        let mut cx = Context::from_parts(counter, &entities, &globals, &callbacks, &notified);

        let listener = cx.listener(Counter::increment);

        let _element = div().id("increment").on_activate(listener).child("+");
    }

    #[test]
    fn stateful_element_preserves_click_after_styling() {
        let entities = EntityArena::<1024, 16>::default();
        let callbacks = CallbackArena::<1024, 16>::default();
        let globals = GlobalArena::<0, 0>::default();
        let counter = entities.insert(Counter { value: 0 }).unwrap();
        let notified = Cell::new(false);

        let mut cx = Context::from_parts(counter, &entities, &globals, &callbacks, &notified);

        let listener = cx.listener(Counter::increment);

        let _element = div()
            .id("increment")
            .on_activate(listener)
            .flex()
            .p(px(8))
            .child("+");
    }

    #[test]
    fn stateful_element_preserves_click_after_adding_children() {
        let entities = EntityArena::<1024, 16>::default();
        let callbacks = CallbackArena::<1024, 16>::default();
        let globals = GlobalArena::<0, 0>::default();
        let counter = entities.insert(Counter { value: 0 }).unwrap();
        let notified = Cell::new(false);

        let mut cx = Context::from_parts(counter, &entities, &globals, &callbacks, &notified);

        let listener = cx.listener(Counter::increment);

        let _element = div()
            .id("increment")
            .on_activate(listener)
            .child("+")
            .child("Increment");
    }

    #[test]
    fn click_listener_can_be_added_after_children_and_styling() {
        let entities = EntityArena::<1024, 16>::default();
        let callbacks = CallbackArena::<1024, 16>::default();
        let globals = GlobalArena::<0, 0>::default();
        let counter = entities.insert(Counter { value: 0 }).unwrap();
        let notified = Cell::new(false);

        let mut cx = Context::from_parts(counter, &entities, &globals, &callbacks, &notified);

        let listener = cx.listener(Counter::increment);

        let _element = div()
            .child("+")
            .id("increment")
            .p(px(8))
            .on_activate(listener);
    }
}
