use core::{
    alloc::Layout,
    any::TypeId,
    cell::{Cell, RefCell, UnsafeCell},
    mem::MaybeUninit,
    ptr::NonNull,
};

use heapless::Vec;

use crate::{
    Context, Entity, EntityAccessError, EntityBorrowKind, EntityId, Listener, ListenerId, align_up,
    entity_store::{EntityStore, RawEntityBorrow},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListenerAllocError {
    SlotsFull,
    StorageFull,
    UnsupportedAlignment { requested: usize, supported: usize },
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

type ListenerInvokeFn = unsafe fn(
    closure: *const u8,
    target: EntityId,
    event: *const u8,
    entities: &dyn EntityStore,
    listeners: &dyn ListenerStore,
    notified: &Cell<bool>,
) -> Result<(), ListenerInvokeError>;

#[derive(Clone, Copy)]
struct ListenerMeta {
    offset: usize,
    target: EntityId,
    event_type: TypeId,
    generation: u32,
    invoke_fn: ListenerInvokeFn,
    drop_fn: unsafe fn(*mut u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ListenerSlotState {
    Vacant,
    Initializing,
    Live,
    Abandonned,
}

pub(crate) struct RawListenerReservation {
    pub(crate) id: ListenerId,
    pub(crate) ptr: NonNull<u8>,
}

/// # Safety
///
/// the reserved storage for `listener` must contain a valid initialized
/// callback of the type registered during `reserve`.
pub(crate) unsafe trait ListenerStore {
    fn reserve(
        &self,
        layout: Layout,
        target: EntityId,
        event_type: TypeId,
        invoke_fn: ListenerInvokeFn,
        drop_fn: unsafe fn(*mut u8),
    ) -> Result<RawListenerReservation, ListenerAllocError>;

    unsafe fn commit(&self, listener: ListenerId);
}

const LISTENER_ARENA_ALIGNMENT: usize = 16;

#[repr(C, align(16))]
struct ListenerStorage<const N: usize> {
    bytes: [MaybeUninit<u8>; N],
}

impl<const N: usize> ListenerStorage<N> {
    const fn new() -> Self {
        Self {
            bytes: [MaybeUninit::uninit(); N],
        }
    }
}

/// SAFETY INVARIANTS:
///
/// 1. every Live [`ListenerMeta`] refers to one initialized callback F.
/// 2. callback storage remains fixed until reset.
/// 3. listener generation must match metadata generation before invocation.
/// 4. event [`TypeId`] is checked before casting event pointer to E.
/// 5. the callback trampoline used for a listener matches the concrete F, T, and E used when
///    that listener was registered.
/// 6. the callback target is exclusively borrowed through [`EntityStore`] for
///    the complete callback.
/// 7. only Live listeners are dropped.
/// 8. each live callback is dropped exactly once.
pub struct ListenerArena<const BYTES: usize, const SLOTS: usize> {
    storage: UnsafeCell<ListenerStorage<BYTES>>,
    entries: RefCell<Vec<ListenerMeta, SLOTS>>,
    states: [Cell<ListenerSlotState>; SLOTS],
    cursor: Cell<usize>,
    generation: Cell<u32>,
}

impl<const BYTES: usize, const SLOTS: usize> Default for ListenerArena<BYTES, SLOTS> {
    fn default() -> Self {
        Self {
            storage: UnsafeCell::new(ListenerStorage::new()),
            entries: RefCell::new(Vec::new()),
            states: core::array::from_fn(|_| Cell::new(ListenerSlotState::Vacant)),
            cursor: Cell::new(0),
            generation: Cell::new(0),
        }
    }
}

impl<const BYTES: usize, const SLOTS: usize> ListenerArena<BYTES, SLOTS> {
    fn storage_ptr(&self) -> *mut u8 {
        let storage = self.storage.get();

        unsafe {
            core::ptr::addr_of_mut!((*storage).bytes)
                .cast::<MaybeUninit<u8>>()
                .cast::<u8>()
        }
    }

    pub fn invoke<E>(
        &self,
        listener: Listener<E>,
        event: &E,
        entities: &dyn EntityStore,
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
        if self.states[slot].get() != ListenerSlotState::Live {
            return Err(ListenerInvokeError::InvalidListener);
        }
        if meta.event_type != TypeId::of::<E>() {
            return Err(ListenerInvokeError::EventTypeMismatch);
        }

        let closure = unsafe { self.storage_ptr().add(meta.offset) };

        unsafe {
            (meta.invoke_fn)(
                closure,
                meta.target,
                event as *const E as *const u8,
                entities,
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
            if self.states[slot].get() != ListenerSlotState::Live {
                continue;
            }

            let callback_ptr = unsafe { storage_ptr.add(meta.offset) };
            unsafe { (meta.drop_fn)(callback_ptr) };

            self.states[slot].set(ListenerSlotState::Abandonned);
        }
    }

    pub fn reset(&mut self) {
        self.drop_live_callbacks();

        self.entries.get_mut().clear();

        for state in &self.states {
            state.set(ListenerSlotState::Vacant);
        }

        self.cursor.set(0);
        self.generation.set(self.generation.get().wrapping_add(1));
    }
}

unsafe impl<const BYTES: usize, const SLOTS: usize> ListenerStore for ListenerArena<BYTES, SLOTS> {
    fn reserve(
        &self,
        layout: Layout,
        target: EntityId,
        event_type: TypeId,
        invoke_fn: ListenerInvokeFn,
        drop_fn: unsafe fn(*mut u8),
    ) -> Result<RawListenerReservation, ListenerAllocError> {
        let mut entries = self.entries.borrow_mut();
        let slot = entries.len();

        if slot >= SLOTS || slot > u16::MAX as usize {
            return Err(ListenerAllocError::SlotsFull);
        }

        let alignment = layout.align();
        if alignment > LISTENER_ARENA_ALIGNMENT {
            return Err(ListenerAllocError::UnsupportedAlignment {
                requested: alignment,
                supported: LISTENER_ARENA_ALIGNMENT,
            });
        }

        let offset =
            align_up(self.cursor.get(), alignment).ok_or(ListenerAllocError::StorageFull)?;

        // reserve at least one byte for ZST so separate entries still receive
        // distinct storage locations
        let allocation_size = layout.size().max(1);

        let end = offset
            .checked_add(allocation_size)
            .ok_or(ListenerAllocError::StorageFull)?;
        if end > BYTES {
            return Err(ListenerAllocError::StorageFull);
        }

        let generation = self.generation.get();
        let id = ListenerId::new(slot as u16, generation);

        let meta = ListenerMeta {
            offset,
            target,
            event_type,
            generation,
            invoke_fn,
            drop_fn,
        };

        entries
            .push(meta)
            .map_err(|_| ListenerAllocError::SlotsFull)?;

        self.cursor.set(end);
        self.states[slot].set(ListenerSlotState::Initializing);

        let ptr = unsafe { self.storage_ptr().add(offset) };
        let ptr = unsafe { NonNull::new_unchecked(ptr) };

        Ok(RawListenerReservation { id, ptr })
    }

    unsafe fn commit(&self, listener: ListenerId) {
        let slot = listener.slot() as usize;
        self.states[slot].set(ListenerSlotState::Live);
    }
}

impl<const BYTES: usize, const SLOTS: usize> Drop for ListenerArena<BYTES, SLOTS> {
    fn drop(&mut self) {
        self.drop_live_callbacks();
    }
}

unsafe fn drop_listener<F>(ptr: *mut u8) {
    unsafe { core::ptr::drop_in_place(ptr.cast::<F>()) };
}

unsafe fn invoke_listener<T, E, F>(
    closure: *const u8,
    target: EntityId,
    event: *const u8,
    entities: &dyn EntityStore,
    listeners: &dyn ListenerStore,
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
    let mut cx = Context::from_parts(entity, entities, listeners, notified);

    callback(state, event, &mut cx);

    Ok(())
}

pub(crate) fn register_listener<T, E, F>(
    store: &dyn ListenerStore,
    entity: Entity<T>,
    callback: F,
) -> Result<Listener<E>, ListenerAllocError>
where
    T: 'static,
    E: 'static,
    F: Fn(&mut T, &E, &mut Context<'_, T>) + 'static,
{
    let reservation = store.reserve(
        Layout::new::<F>(),
        entity.entity_id(),
        TypeId::of::<E>(),
        invoke_listener::<T, E, F>,
        drop_listener::<F>,
    )?;

    unsafe { reservation.ptr.cast::<F>().as_ptr().write(callback) };
    unsafe { store.commit(reservation.id) };

    Ok(Listener::from_id(reservation.id))
}

#[cfg(test)]
mod tests {
    use core::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::*;

    struct Counter {
        value: i32,
    }

    impl Counter {
        fn increment(&mut self, _: &ClickEvent, cx: &mut Context<Self>) {
            self.value += 1;
            cx.notify();
        }
    }

    #[test]
    fn invokes_method_listener() {
        let entities = EntityArena::<1024, 16>::default();
        let listeners = ListenerArena::<1024, 16>::default();

        let root = entities.insert(Counter { value: 0 }).unwrap();

        let notified = Cell::new(false);

        let listener = {
            let mut cx = Context::from_parts(root, &entities, &listeners, &notified);
            cx.listener(Counter::increment)
        };

        listeners
            .invoke(listener, &ClickEvent, &entities, &notified)
            .unwrap();

        assert_eq!(entities.read(root, |counter| { counter.value }), Ok(1));
        assert!(notified.get());
    }

    #[test]
    fn invokes_capturing_listener() {
        let entities = EntityArena::<1024, 16>::default();
        let listeners = ListenerArena::<1024, 16>::default();
        let counter = entities.insert(Counter { value: 0 }).unwrap();
        let notified = Cell::new(false);
        let amount = 5;

        let listener = {
            let mut cx = Context::from_parts(counter, &entities, &listeners, &notified);

            cx.listener(move |counter: &mut Counter, _: &ClickEvent, cx| {
                counter.value += amount;
                cx.notify();
            })
        };

        listeners
            .invoke(listener, &ClickEvent, &entities, &notified)
            .unwrap();

        assert_eq!(entities.read(counter, |counter| counter.value), Ok(5));
        assert!(notified.get());
    }

    #[test]
    fn listener_rejects_reentrant_update_of_target_entity() {
        let entities = EntityArena::<1024, 16>::default();
        let listeners = ListenerArena::<1024, 16>::default();
        let counter = entities.insert(Counter { value: 0 }).unwrap();
        let notified = Cell::new(false);

        let listener = {
            let mut cx = Context::from_parts(counter, &entities, &listeners, &notified);

            cx.listener(move |_: &mut Counter, _: &ClickEvent, cx| {
                let result = counter.update(cx, |_, _| {});

                assert!(matches!(result, Err(EntityAccessError::BorrowConflict)));
            })
        };

        listeners
            .invoke(listener, &ClickEvent, &entities, &notified)
            .unwrap();
    }

    struct Settings {
        dirty: bool,
    }

    #[test]
    fn listener_can_update_another_entity() {
        let entities = EntityArena::<1024, 16>::default();
        let listeners = ListenerArena::<1024, 16>::default();
        let counter = entities.insert(Counter { value: 0 }).unwrap();
        let settings = entities.insert(Settings { dirty: false }).unwrap();
        let notified = Cell::new(false);

        let listener = {
            let mut cx = Context::from_parts(counter, &entities, &listeners, &notified);

            cx.listener(move |counter: &mut Counter, _: &ClickEvent, cx| {
                counter.value += 1;

                settings
                    .update(cx, |settings, cx| {
                        settings.dirty = true;
                        cx.notify();
                    })
                    .unwrap();
            })
        };

        listeners
            .invoke(listener, &ClickEvent, &entities, &notified)
            .unwrap();

        assert_eq!(entities.read(counter, |counter| counter.value), Ok(1));
        assert!(entities.read(settings, |settings| settings.dirty).unwrap());
        assert!(notified.get());
    }

    #[test]
    fn stale_listener_is_rejected_after_reset() {
        let entities = EntityArena::<1024, 16>::default();
        let mut listeners = ListenerArena::<1024, 16>::default();
        let counter = entities.insert(Counter { value: 0 }).unwrap();
        let notified = Cell::new(false);

        let listener = {
            let mut cx = Context::from_parts(counter, &entities, &listeners, &notified);

            cx.listener(Counter::increment)
        };

        listeners.reset();

        let result = listeners.invoke(listener, &ClickEvent, &entities, &notified);

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
        let mut listeners = ListenerArena::<1024, 16>::default();
        let counter = entities.insert(Counter { value: 0 }).unwrap();
        let notified = Cell::new(false);

        {
            let capture = DroppableCapture;

            let mut cx = Context::from_parts(counter, &entities, &listeners, &notified);

            let _listener = cx.listener(move |_: &mut Counter, _: &ClickEvent, _| {
                let _ = &capture;
            });
        }

        assert_eq!(DROPS.load(Ordering::SeqCst), 0);

        listeners.reset();

        assert_eq!(DROPS.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn listener_arena_reports_slot_exhaustion() {
        let entities = EntityArena::<1024, 16>::default();
        let listeners = ListenerArena::<1024, 1>::default();
        let counter = entities.insert(Counter { value: 0 }).unwrap();
        let notified = Cell::new(false);

        let mut cx = Context::from_parts(counter, &entities, &listeners, &notified);

        cx.try_listener::<ClickEvent, _>(|_: &mut Counter, _, _| {})
            .unwrap();

        let result = cx.try_listener::<ClickEvent, _>(|_: &mut Counter, _, _| {});

        assert!(matches!(result, Err(ListenerAllocError::SlotsFull)));
    }

    #[test]
    fn listener_arena_reports_storage_exhaustion() {
        let entities = EntityArena::<1024, 16>::default();
        let listeners = ListenerArena::<4, 16>::default();
        let counter = entities.insert(Counter { value: 0 }).unwrap();
        let notified = Cell::new(false);
        let capture = [0u8; 32];

        let mut cx = Context::from_parts(counter, &entities, &listeners, &notified);

        let result = cx.try_listener::<ClickEvent, _>(move |_: &mut Counter, _, _| {
            let _ = &capture;
        });

        assert!(matches!(result, Err(ListenerAllocError::StorageFull)));
    }

    #[test]
    fn click_listener_can_be_attached_to_stateful_div() {
        let entities = EntityArena::<1024, 16>::default();
        let listeners = ListenerArena::<1024, 16>::default();
        let counter = entities.insert(Counter { value: 0 }).unwrap();
        let notified = Cell::new(false);

        let mut cx = Context::from_parts(counter, &entities, &listeners, &notified);

        let listener = cx.listener(Counter::increment);

        let _element = div().id("increment").on_click(listener).child("+");
    }

    #[test]
    fn stateful_element_preserves_click_after_styling() {
        let entities = EntityArena::<1024, 16>::default();
        let listeners = ListenerArena::<1024, 16>::default();
        let counter = entities.insert(Counter { value: 0 }).unwrap();
        let notified = Cell::new(false);

        let mut cx = Context::from_parts(counter, &entities, &listeners, &notified);

        let listener = cx.listener(Counter::increment);

        let _element = div()
            .id("increment")
            .on_click(listener)
            .flex()
            .p(px(8))
            .child("+");
    }

    #[test]
    fn stateful_element_preserves_click_after_adding_children() {
        let entities = EntityArena::<1024, 16>::default();
        let listeners = ListenerArena::<1024, 16>::default();
        let counter = entities.insert(Counter { value: 0 }).unwrap();
        let notified = Cell::new(false);

        let mut cx = Context::from_parts(counter, &entities, &listeners, &notified);

        let listener = cx.listener(Counter::increment);

        let _element = div()
            .id("increment")
            .on_click(listener)
            .child("+")
            .child("Increment");
    }

    #[test]
    fn click_listener_can_be_added_after_children_and_styling() {
        let entities = EntityArena::<1024, 16>::default();
        let listeners = ListenerArena::<1024, 16>::default();
        let counter = entities.insert(Counter { value: 0 }).unwrap();
        let notified = Cell::new(false);

        let mut cx = Context::from_parts(counter, &entities, &listeners, &notified);

        let listener = cx.listener(Counter::increment);

        let _element = div().child("+").id("increment").p(px(8)).on_click(listener);
    }
}
