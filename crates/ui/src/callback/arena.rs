use core::{alloc::Layout, any::TypeId, cell::Cell, ptr::NonNull};

#[cfg(feature = "alloc")]
use crate::slot_table::HeapSlots;
use crate::{
    BorrowKind, Context, Entity, EntityAccessError, EntityId, Listener, PaintCx,
    callback::CallbackId,
    entity::{EntityStore, RawEntityBorrow},
    global::GlobalStore,
    slot_table::{FixedSlots, ReserveError, SlotStorage, SlotTable},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallbackAllocError {
    SlotsFull,
    StorageFull,
    UnsupportedAlignment {
        requested: usize,
        supported: usize,
    },
    /// heap-backed storage could not allocate
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
pub enum CanvasInvokeError {
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
    paint: &mut PaintCx<'_>,
    entities: &dyn EntityStore,
) -> Result<(), CanvasInvokeError>;

#[derive(Clone, Copy)]
pub enum CallbackKind {
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
    target: EntityId,
    kind: CallbackKind,
}

pub struct RawCallbackReservation {
    pub(crate) id: CallbackId,
    pub(crate) ptr: NonNull<u8>,
}

/// # Safety
///
/// the reserved storage for `callback` must contain a valid initialized
/// callback of the type registered during `reserve`.
pub unsafe trait CallbackStore {
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
        paint: &mut PaintCx<'_>,
        entities: &dyn EntityStore,
    ) -> Result<(), CanvasInvokeError>;
}

impl From<ReserveError> for CallbackAllocError {
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

/// fixed-capacity callback storage: at most `SLOTS` callbacks in `BYTES` bytes per frame.
/// Never allocates.
pub type FixedCallbackArena<const BYTES: usize, const SLOTS: usize> =
    CallbackArena<FixedSlots<BYTES, SLOTS>>;

/// heap callback storage. The first allocation reserves `INITIAL_SLOTS` callbacks.
#[cfg(feature = "alloc")]
pub type HeapCallbackArena<const INITIAL_SLOTS: usize = 0> =
    CallbackArena<HeapSlots<INITIAL_SLOTS>>;

/// the callbacks registered while rendering one frame, stored by [`SlotTable`].
///
/// SAFETY INVARIANTS:
///
/// 1. callback ids carry the generation of the frame that registered them, and only
///    match while that frame's callbacks are stored.
/// 2. event [`TypeId`] is checked before casting event pointer to E.
/// 3. the callback trampoline used for a callback matches the concrete F, T, and E used when
///    that callback was registered.
/// 4. Listeners hold an exclusive target borrow and canvas callbacks hold a shared target
///    borrow for the complete callback
/// 5. invocation copies metadata out before calling user code, so callbacks may register
///    more callbacks. Reset takes `&mut self`, so it can't run during an invocation.
pub struct CallbackArena<S: SlotStorage> {
    slots: SlotTable<S, CallbackMeta>,
    generation: u32,
}

impl<S: SlotStorage> Default for CallbackArena<S> {
    fn default() -> Self {
        Self {
            slots: SlotTable::default(),
            generation: 0,
        }
    }
}

impl<S: SlotStorage> CallbackArena<S> {
    fn lookup(&self, id: CallbackId) -> Option<(CallbackMeta, NonNull<u8>)> {
        if id.generation() != self.generation {
            return None;
        }

        self.slots.live(usize::from(id.slot())).ok()
    }

    pub(crate) fn invoke_listener<E>(
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
        let (meta, closure) = self
            .lookup(listener.id)
            .ok_or(ListenerInvokeError::InvalidListener)?;

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

        // SAFETY: lookup validated a live closure of this frame, and the event type
        // matches its trampoline
        unsafe {
            invoke_fn(
                closure.as_ptr(),
                meta.target,
                event as *const E as *const u8,
                entities,
                globals,
                self,
                notified,
            )
        }
    }

    pub(crate) fn reset(&mut self) {
        // invalidate handles before dropping captures, including if a drop unwinds
        self.generation = self.generation.wrapping_add(1);
        self.slots.clear();
    }
}

// SAFETY: reservations fit the registered closure. Only committed closures are
// callable. Both invocation paths validate generation and kind and use the registered
// trampoline, which enforces the target entity's borrow rules
unsafe impl<S: SlotStorage> CallbackStore for CallbackArena<S> {
    fn reserve(
        &self,
        layout: Layout,
        target: EntityId,
        kind: CallbackKind,
        drop_fn: unsafe fn(*mut u8),
    ) -> Result<RawCallbackReservation, CallbackAllocError> {
        let (slot, ptr) = self
            .slots
            .reserve(layout, CallbackMeta { target, kind }, drop_fn)?;

        Ok(RawCallbackReservation {
            id: CallbackId::new(slot, self.generation),
            ptr,
        })
    }

    unsafe fn commit(&self, callback: CallbackId) {
        debug_assert_eq!(callback.generation(), self.generation);

        // SAFETY: the caller initialized the reserved closure
        unsafe { self.slots.commit(callback.slot()) };
    }

    fn invoke_canvas(
        &self,
        callback: CallbackId,
        paint: &mut PaintCx<'_>,
        entities: &dyn EntityStore,
    ) -> Result<(), CanvasInvokeError> {
        let (meta, closure) = self
            .lookup(callback)
            .ok_or(CanvasInvokeError::InvalidCallback)?;

        let CallbackKind::Canvas { invoke_fn } = meta.kind else {
            return Err(CanvasInvokeError::CallbackKindMismatch);
        };

        // SAFETY: lookup validated a live closure of this frame. Its canvas trampoline
        // acquires a shared borrow of the target
        unsafe { invoke_fn(closure.as_ptr(), meta.target, paint, entities) }
    }
}

impl<S: SlotStorage> crate::storage::CallbackStorage for CallbackArena<S> {
    fn reset(&mut self) {
        CallbackArena::reset(self);
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
        CallbackArena::invoke_listener(self, listener, event, entities, globals, notified)
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
    let borrow =
        RawEntityBorrow::acquire(entities, target, TypeId::of::<T>(), BorrowKind::Exclusive)?;

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
    paint: &mut PaintCx<'_>,
    entities: &dyn EntityStore,
) -> Result<(), CanvasInvokeError>
where
    T: 'static,
    F: Fn(&T, &mut PaintCx<'_>) + 'static,
{
    let borrow = RawEntityBorrow::acquire(entities, target, TypeId::of::<T>(), BorrowKind::Shared)?;

    let state = unsafe { &*borrow.ptr().cast::<T>().as_ptr() };
    let callback = unsafe { &*closure.cast::<F>() };

    callback(state, paint);

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
    F: Fn(&T, &mut PaintCx<'_>) + 'static,
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

#[cfg(all(test, feature = "alloc"))]
#[path = "heap_tests.rs"]
mod heap_tests;

#[cfg(test)]
mod tests {
    use core::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
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
        let entities = TestEntityArena::<1024, 16>::default();
        let callbacks = FixedCallbackArena::<1024, 16>::default();
        let globals = TestGlobalArena::<0, 0>::default();

        let root = entities.insert(Counter { value: 0 }).unwrap();

        let notified = Cell::new(false);

        let listener = {
            let mut cx = Context::from_parts(root, &entities, &globals, &callbacks, &notified);
            cx.listener(Counter::increment)
        };

        callbacks
            .invoke_listener(
                listener,
                &ActivateEvent::new(ElementId::Name("test")),
                &entities,
                &globals,
                &notified,
            )
            .unwrap();

        assert_eq!(entities.read(root, |counter| { counter.value }), Ok(1));
        assert!(notified.get());
    }

    #[test]
    fn invokes_capturing_listener() {
        let entities = TestEntityArena::<1024, 16>::default();
        let callbacks = FixedCallbackArena::<1024, 16>::default();
        let globals = TestGlobalArena::<0, 0>::default();
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
            .invoke_listener(
                listener,
                &ActivateEvent::new(ElementId::Name("test")),
                &entities,
                &globals,
                &notified,
            )
            .unwrap();

        assert_eq!(entities.read(counter, |counter| counter.value), Ok(5));
        assert!(notified.get());
    }

    #[test]
    fn listener_rejects_reentrant_update_of_target_entity() {
        let entities = TestEntityArena::<1024, 16>::default();
        let callbacks = FixedCallbackArena::<1024, 16>::default();
        let globals = TestGlobalArena::<0, 0>::default();
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
            .invoke_listener(
                listener,
                &ActivateEvent::new(ElementId::Name("test")),
                &entities,
                &globals,
                &notified,
            )
            .unwrap();
    }

    struct Settings {
        dirty: bool,
    }

    #[test]
    fn listener_can_update_another_entity() {
        let entities = TestEntityArena::<1024, 16>::default();
        let callbacks = FixedCallbackArena::<1024, 16>::default();
        let globals = TestGlobalArena::<0, 0>::default();
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
            .invoke_listener(
                listener,
                &ActivateEvent::new(ElementId::Name("test")),
                &entities,
                &globals,
                &notified,
            )
            .unwrap();

        assert_eq!(entities.read(counter, |counter| counter.value), Ok(1));
        assert!(entities.read(settings, |settings| settings.dirty).unwrap());
        assert!(notified.get());
    }

    #[test]
    fn stale_listener_is_rejected_after_reset() {
        let entities = TestEntityArena::<1024, 16>::default();
        let mut callbacks = FixedCallbackArena::<1024, 16>::default();
        let globals = TestGlobalArena::<0, 0>::default();
        let counter = entities.insert(Counter { value: 0 }).unwrap();
        let notified = Cell::new(false);

        let listener = {
            let mut cx = Context::from_parts(counter, &entities, &globals, &callbacks, &notified);

            cx.listener(Counter::increment)
        };

        callbacks.reset();

        let result = callbacks.invoke_listener(
            listener,
            &ActivateEvent::new(ElementId::Name("test")),
            &entities,
            &globals,
            &notified,
        );

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

        let entities = TestEntityArena::<1024, 16>::default();
        let mut callbacks = FixedCallbackArena::<1024, 16>::default();
        let globals = TestGlobalArena::<0, 0>::default();
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
    fn listener_arena_reports_slot_exhaustion() {
        let entities = TestEntityArena::<1024, 16>::default();
        let callbacks = FixedCallbackArena::<1024, 1>::default();
        let globals = TestGlobalArena::<0, 0>::default();
        let counter = entities.insert(Counter { value: 0 }).unwrap();
        let notified = Cell::new(false);

        let mut cx = Context::from_parts(counter, &entities, &globals, &callbacks, &notified);

        cx.try_listener::<ActivateEvent, _>(|_: &mut Counter, _, _| {})
            .unwrap();

        let result = cx.try_listener::<ActivateEvent, _>(|_: &mut Counter, _, _| {});

        assert!(matches!(result, Err(CallbackAllocError::SlotsFull)));
    }

    #[test]
    fn listener_arena_reports_storage_exhaustion() {
        let entities = TestEntityArena::<1024, 16>::default();
        let callbacks = FixedCallbackArena::<4, 16>::default();
        let globals = TestGlobalArena::<0, 0>::default();
        let counter = entities.insert(Counter { value: 0 }).unwrap();
        let notified = Cell::new(false);
        let capture = [0u8; 32];

        let mut cx = Context::from_parts(counter, &entities, &globals, &callbacks, &notified);

        let result = cx.try_listener::<ActivateEvent, _>(move |_: &mut Counter, _, _| {
            let _ = &capture;
        });

        assert!(matches!(result, Err(CallbackAllocError::StorageFull)));
    }
}
