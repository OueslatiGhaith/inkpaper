use core::{alloc::Layout, any::TypeId, ptr::NonNull};

#[cfg(feature = "alloc")]
use crate::slot_table::HeapSlots;
use crate::{
    BorrowKind, Context, Entity, EntityAccessError, EntityId, Listener, PaintCx, RuntimeCx,
    callback::CallbackId,
    entity::{EntityStore, RawEntityBorrow},
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
    runtime: RuntimeCx<'_>,
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
    /// the entity whose render registered the callback, or the target outside rendering.
    /// The callback is released when its owner renders again
    owner: EntityId,
    target: EntityId,
    kind: CallbackKind,
}

/// the callbacks registered so far. Slot numbers grow until the arena resets, so later
/// callbacks have larger slots
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CallbackMark(usize);

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
        owner: EntityId,
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
///    match while that frame's callbacks are stored and their slot is live. Released
///    callbacks keep their slot number until reset, so their ids stay invalid.
/// 2. event [`TypeId`] is checked before casting event pointer to E.
/// 3. the callback trampoline used for a callback matches the concrete F, T, and E used when
///    that callback was registered.
/// 4. Listeners hold an exclusive target borrow and canvas callbacks hold a shared target
///    borrow for the complete callback
/// 5. invocation copies metadata out before calling user code, so callbacks may register
///    more callbacks. Reset and release take `&mut self`, so they can't run during an
///    invocation.
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

    /// runs a listener of this arena. `runtime` must refer to this arena as its callbacks
    pub(crate) fn invoke_listener<E>(
        &self,
        listener: Listener<E>,
        event: &E,
        runtime: RuntimeCx<'_>,
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
                runtime,
            )
        }
    }

    pub(crate) fn reset(&mut self) {
        // invalidate handles before dropping captures, including if a drop unwinds
        self.generation = self.generation.wrapping_add(1);
        self.slots.clear();
    }

    /// a mark that [`Self::release_owner`] can keep the callbacks registered after
    pub(crate) fn mark(&self) -> CallbackMark {
        CallbackMark(self.slots.len())
    }

    /// drops the callbacks `owner` registered before `mark`, such as during its previous
    /// render. Other callbacks stay callable. Returns how many callbacks were released.
    pub(crate) fn release_owner(&mut self, owner: EntityId, mark: CallbackMark) -> usize {
        self.slots
            .remove_where(|slot, meta| slot < mark.0 && meta.owner == owner)
    }
}

// SAFETY: reservations fit the registered closure. Only committed closures are
// callable. Both invocation paths validate generation and kind and use the registered
// trampoline, which enforces the target entity's borrow rules
unsafe impl<S: SlotStorage> CallbackStore for CallbackArena<S> {
    fn reserve(
        &self,
        layout: Layout,
        owner: EntityId,
        target: EntityId,
        kind: CallbackKind,
        drop_fn: unsafe fn(*mut u8),
    ) -> Result<RawCallbackReservation, CallbackAllocError> {
        let meta = CallbackMeta {
            owner,
            target,
            kind,
        };
        let (slot, ptr) = self.slots.reserve(layout, meta, drop_fn)?;

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

    fn mark(&self) -> CallbackMark {
        CallbackArena::mark(self)
    }

    fn release_owner(&mut self, owner: EntityId, mark: CallbackMark) -> usize {
        CallbackArena::release_owner(self, owner, mark)
    }

    fn invoke_listener<E>(
        &self,
        listener: Listener<E>,
        event: &E,
        runtime: RuntimeCx<'_>,
    ) -> Result<(), ListenerInvokeError>
    where
        E: 'static,
    {
        CallbackArena::invoke_listener(self, listener, event, runtime)
    }
}

unsafe fn drop_callback<F>(ptr: *mut u8) {
    unsafe { core::ptr::drop_in_place(ptr.cast::<F>()) };
}

unsafe fn invoke_listener_callback<T, E, F>(
    closure: *const u8,
    target: EntityId,
    event: *const u8,
    runtime: RuntimeCx<'_>,
) -> Result<(), ListenerInvokeError>
where
    T: 'static,
    E: 'static,
    F: Fn(&mut T, &E, &mut Context<'_, T>) + 'static,
{
    let borrow = RawEntityBorrow::acquire(
        runtime.entities,
        target,
        TypeId::of::<T>(),
        BorrowKind::Exclusive,
    )?;

    let state = unsafe { &mut *borrow.ptr().cast::<T>().as_ptr() };
    let event = unsafe { &*event.cast::<E>() };
    let callback = unsafe { &*closure.cast::<F>() };
    let entity = Entity::<T>::from_id(target);
    let mut cx = Context::new_in(entity, runtime);

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

/// registers a listener on `entity`, released when `owner` renders again
pub(crate) fn register_listener<T, E, F>(
    store: &dyn CallbackStore,
    owner: EntityId,
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
        owner,
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

/// registers a canvas callback on `entity`, released when `owner` renders again
pub(crate) fn register_canvas_callback<T, F>(
    store: &dyn CallbackStore,
    owner: EntityId,
    entity: Entity<T>,
    callback: F,
) -> Result<CallbackId, CallbackAllocError>
where
    T: 'static,
    F: Fn(&T, &mut PaintCx<'_>) + 'static,
{
    let reservation = store.reserve(
        Layout::new::<F>(),
        owner,
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
    use core::{
        cell::Cell,
        sync::atomic::{AtomicUsize, Ordering},
    };

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

        let all_dirty = Cell::new(false);

        let listener = {
            let mut cx = Context::new_in(
                root,
                RuntimeCx::new(&entities, &globals, &callbacks, &all_dirty),
            );
            cx.listener(Counter::increment)
        };

        callbacks
            .invoke_listener(
                listener,
                &ActivateEvent::new(ElementId::Name("test")),
                RuntimeCx::new(&entities, &globals, &callbacks, &all_dirty),
            )
            .unwrap();

        assert_eq!(entities.read(root, |counter| { counter.value }), Ok(1));
        assert!(entities.is_dirty(root.entity_id()));
    }

    #[test]
    fn invokes_capturing_listener() {
        let entities = TestEntityArena::<1024, 16>::default();
        let callbacks = FixedCallbackArena::<1024, 16>::default();
        let globals = TestGlobalArena::<0, 0>::default();
        let counter = entities.insert(Counter { value: 0 }).unwrap();
        let all_dirty = Cell::new(false);
        let amount = 5;

        let listener = {
            let mut cx = Context::new_in(
                counter,
                RuntimeCx::new(&entities, &globals, &callbacks, &all_dirty),
            );

            cx.listener(move |counter: &mut Counter, _: &ActivateEvent, cx| {
                counter.value += amount;
                cx.notify();
            })
        };

        callbacks
            .invoke_listener(
                listener,
                &ActivateEvent::new(ElementId::Name("test")),
                RuntimeCx::new(&entities, &globals, &callbacks, &all_dirty),
            )
            .unwrap();

        assert_eq!(entities.read(counter, |counter| counter.value), Ok(5));
        assert!(entities.is_dirty(counter.entity_id()));
    }

    #[test]
    fn listener_rejects_reentrant_update_of_target_entity() {
        let entities = TestEntityArena::<1024, 16>::default();
        let callbacks = FixedCallbackArena::<1024, 16>::default();
        let globals = TestGlobalArena::<0, 0>::default();
        let counter = entities.insert(Counter { value: 0 }).unwrap();
        let all_dirty = Cell::new(false);

        let listener = {
            let mut cx = Context::new_in(
                counter,
                RuntimeCx::new(&entities, &globals, &callbacks, &all_dirty),
            );

            cx.listener(move |_: &mut Counter, _: &ActivateEvent, cx| {
                let result = counter.update(cx, |_, _| {});

                assert!(matches!(result, Err(EntityAccessError::BorrowConflict)));
            })
        };

        callbacks
            .invoke_listener(
                listener,
                &ActivateEvent::new(ElementId::Name("test")),
                RuntimeCx::new(&entities, &globals, &callbacks, &all_dirty),
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
        let all_dirty = Cell::new(false);

        let listener = {
            let mut cx = Context::new_in(
                counter,
                RuntimeCx::new(&entities, &globals, &callbacks, &all_dirty),
            );

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
                RuntimeCx::new(&entities, &globals, &callbacks, &all_dirty),
            )
            .unwrap();

        assert_eq!(entities.read(counter, |counter| counter.value), Ok(1));
        assert!(entities.read(settings, |settings| settings.dirty).unwrap());
        // only the entity that notified needs to render again
        assert!(entities.is_dirty(settings.entity_id()));
        assert!(!entities.is_dirty(counter.entity_id()));
    }

    #[test]
    fn stale_listener_is_rejected_after_reset() {
        let entities = TestEntityArena::<1024, 16>::default();
        let mut callbacks = FixedCallbackArena::<1024, 16>::default();
        let globals = TestGlobalArena::<0, 0>::default();
        let counter = entities.insert(Counter { value: 0 }).unwrap();
        let all_dirty = Cell::new(false);

        let listener = {
            let mut cx = Context::new_in(
                counter,
                RuntimeCx::new(&entities, &globals, &callbacks, &all_dirty),
            );

            cx.listener(Counter::increment)
        };

        callbacks.reset();

        let result = callbacks.invoke_listener(
            listener,
            &ActivateEvent::new(ElementId::Name("test")),
            RuntimeCx::new(&entities, &globals, &callbacks, &all_dirty),
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
        let all_dirty = Cell::new(false);

        {
            let capture = DroppableCapture;

            let mut cx = Context::new_in(
                counter,
                RuntimeCx::new(&entities, &globals, &callbacks, &all_dirty),
            );

            let _listener = cx.listener(move |_: &mut Counter, _: &ActivateEvent, _| {
                let _ = &capture;
            });
        }

        assert_eq!(DROPS.load(Ordering::SeqCst), 0);

        callbacks.reset();

        assert_eq!(DROPS.load(Ordering::SeqCst), 1);
    }

    static RELEASE_DROPS: AtomicUsize = AtomicUsize::new(0);

    struct ReleasedCapture;

    impl Drop for ReleasedCapture {
        fn drop(&mut self) {
            RELEASE_DROPS.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn releasing_an_owner_keeps_other_callbacks_callable() {
        RELEASE_DROPS.store(0, Ordering::SeqCst);

        let entities = TestEntityArena::<1024, 16>::default();
        let mut callbacks = FixedCallbackArena::<1024, 16>::default();
        let globals = TestGlobalArena::<0, 0>::default();
        let kept = entities.insert(Counter { value: 0 }).unwrap();
        let released = entities.insert(Counter { value: 0 }).unwrap();
        let all_dirty = Cell::new(false);
        let runtime = RuntimeCx::new(&entities, &globals, &callbacks, &all_dirty);

        let kept_listener = Context::new_in(kept, runtime).listener(Counter::increment);
        let released_listener = {
            let capture = ReleasedCapture;

            Context::new_in(released, runtime).listener(
                move |counter: &mut Counter, _: &ActivateEvent, _| {
                    let _ = &capture;
                    counter.value += 1;
                },
            )
        };

        assert_eq!(
            callbacks.release_owner(released.entity_id(), callbacks.mark()),
            1
        );
        // the capture is dropped right away, not when the arena resets
        assert_eq!(RELEASE_DROPS.load(Ordering::SeqCst), 1);

        let event = ActivateEvent::new(ElementId::Name("test"));
        let runtime = RuntimeCx::new(&entities, &globals, &callbacks, &all_dirty);

        assert!(matches!(
            callbacks.invoke_listener(released_listener, &event, runtime),
            Err(ListenerInvokeError::InvalidListener)
        ));
        callbacks
            .invoke_listener(kept_listener, &event, runtime)
            .unwrap();
        assert_eq!(entities.read(kept, |counter| counter.value), Ok(1));

        // a new render gets a new slot, so the released id stays invalid
        let replacement = Context::new_in(released, runtime).listener(Counter::increment);

        assert_ne!(replacement.id, released_listener.id);
        assert!(matches!(
            callbacks.invoke_listener(released_listener, &event, runtime),
            Err(ListenerInvokeError::InvalidListener)
        ));

        callbacks.reset();
        assert_eq!(RELEASE_DROPS.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn releasing_an_owner_keeps_its_callbacks_registered_after_the_mark() {
        let entities = TestEntityArena::<1024, 16>::default();
        let mut callbacks = FixedCallbackArena::<1024, 16>::default();
        let globals = TestGlobalArena::<0, 0>::default();
        let counter = entities.insert(Counter { value: 0 }).unwrap();
        let all_dirty = Cell::new(false);
        let runtime = RuntimeCx::new(&entities, &globals, &callbacks, &all_dirty);

        let previous = Context::new_in(counter, runtime).listener(Counter::increment);
        // the entity renders again after the mark
        let mark = callbacks.mark();
        let current = Context::new_in(counter, runtime).listener(Counter::increment);

        assert_eq!(callbacks.release_owner(counter.entity_id(), mark), 1);

        let event = ActivateEvent::new(ElementId::Name("test"));
        let runtime = RuntimeCx::new(&entities, &globals, &callbacks, &all_dirty);

        assert!(matches!(
            callbacks.invoke_listener(previous, &event, runtime),
            Err(ListenerInvokeError::InvalidListener)
        ));
        callbacks.invoke_listener(current, &event, runtime).unwrap();
    }

    #[test]
    fn callbacks_registered_while_rendering_belong_to_the_rendering_entity() {
        let entities = TestEntityArena::<1024, 16>::default();
        let mut callbacks = FixedCallbackArena::<1024, 16>::default();
        let globals = TestGlobalArena::<0, 0>::default();
        let parent = entities.insert(Counter { value: 0 }).unwrap();
        let child = entities.insert(Counter { value: 0 }).unwrap();
        let all_dirty = Cell::new(false);

        let (listener, _canvas) = {
            let runtime = RuntimeCx::new(&entities, &globals, &callbacks, &all_dirty)
                .rendering(parent.entity_id());
            let cx = Context::new_in(parent, runtime);

            // the parent's render mounts a listener and a canvas that target the child
            child
                .update(&cx, |_, cx| {
                    (cx.listener(Counter::increment), cx.canvas(|_, _| {}))
                })
                .unwrap()
        };

        // outside rendering, the target owns the callback
        let update_listener = Context::new_in(
            child,
            RuntimeCx::new(&entities, &globals, &callbacks, &all_dirty),
        )
        .listener(Counter::increment);

        // the child releases only the callback it registered itself. The parent releases
        // the listener and the canvas
        assert_eq!(
            callbacks.release_owner(child.entity_id(), callbacks.mark()),
            1
        );
        assert_eq!(
            callbacks.release_owner(parent.entity_id(), callbacks.mark()),
            2
        );

        let event = ActivateEvent::new(ElementId::Name("test"));
        let runtime = RuntimeCx::new(&entities, &globals, &callbacks, &all_dirty);

        for listener in [listener, update_listener] {
            assert!(matches!(
                callbacks.invoke_listener(listener, &event, runtime),
                Err(ListenerInvokeError::InvalidListener)
            ));
        }
    }

    #[test]
    fn listener_arena_reports_slot_exhaustion() {
        let entities = TestEntityArena::<1024, 16>::default();
        let callbacks = FixedCallbackArena::<1024, 1>::default();
        let globals = TestGlobalArena::<0, 0>::default();
        let counter = entities.insert(Counter { value: 0 }).unwrap();
        let all_dirty = Cell::new(false);

        let mut cx = Context::new_in(
            counter,
            RuntimeCx::new(&entities, &globals, &callbacks, &all_dirty),
        );

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
        let all_dirty = Cell::new(false);
        let capture = [0u8; 32];

        let mut cx = Context::new_in(
            counter,
            RuntimeCx::new(&entities, &globals, &callbacks, &all_dirty),
        );

        let result = cx.try_listener::<ActivateEvent, _>(move |_: &mut Counter, _, _| {
            let _ = &capture;
        });

        assert!(matches!(result, Err(CallbackAllocError::StorageFull)));
    }
}
