//! typed events from one entity to the entities that subscribe to it.
//!
//! an entity says what happened with [`Context::emit`], and its subscribers decide
//! what it means. Events wait in a queue until the runtime call that emitted them
//! returns, such as the listener a tap ran, so a subscriber can update the emitter
//! without finding it borrowed. A view uses them to reach the entity that owns it,
//! which then updates the view when it needs to.

use core::{
    any::TypeId,
    cell::RefCell,
    mem::{self, MaybeUninit},
};

use crate::{Context, Entity, EntityAccessError, EntityId, RuntimeCx, storage::VecStorage};

/// marks an entity type as emitting events of type `E`, for [`Context::emit`] and
/// [`Context::subscribe`].
///
/// events are copied into the runtime's queue, so they are small `Copy` values of at
/// most 16 bytes, such as an enum naming what happened
pub trait EventEmitter<E: 'static>: 'static {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventError {
    /// the runtime's subscription table is full
    SubscriptionsFull,
    /// the queue of events waiting for delivery is full
    QueueFull,
    /// heap-backed storage could not allocate
    AllocationFailed,
}

/// a subscriber's handler: its state, the emitter and the event
pub(crate) type Handler<T, U, E> = fn(&mut T, Entity<U>, &E, &mut Context<'_, T>);

type InvokeFn = unsafe fn(
    handler: *const (),
    subscriber: EntityId,
    emitter: EntityId,
    event: *const u8,
    runtime: RuntimeCx<'_>,
) -> Result<(), EntityAccessError>;

const EVENT_BYTES: usize = 16;
const EVENT_ALIGN: usize = 8;

#[derive(Clone, Copy)]
#[repr(C, align(8))]
struct EventPayload([MaybeUninit<u8>; EVENT_BYTES]);

const _: () = assert!(mem::align_of::<EventPayload>() == EVENT_ALIGN);

#[derive(Clone, Copy)]
pub(crate) struct Subscription {
    subscriber: EntityId,
    emitter: EntityId,
    event_type: TypeId,
    /// the subscriber's [`Handler`], erased. `invoke` casts it back
    handler: *const (),
    invoke: InvokeFn,
}

#[derive(Clone, Copy)]
pub(crate) struct PendingEvent {
    emitter: EntityId,
    event_type: TypeId,
    payload: EventPayload,
}

/// the runtime's subscriptions and the events waiting for delivery
pub(crate) trait EventStore {
    fn subscribe(&self, subscription: Subscription) -> Result<(), EventError>;

    fn push(&self, event: PendingEvent) -> Result<(), EventError>;

    /// takes the event emitted first
    fn pop(&self) -> Option<PendingEvent>;

    fn subscription(&self, index: usize) -> Option<Subscription>;
}

/// holds nothing, for contexts outside a runtime. Every subscription and event fails
pub(crate) struct NoEvents;

impl EventStore for NoEvents {
    fn subscribe(&self, _: Subscription) -> Result<(), EventError> {
        Err(EventError::SubscriptionsFull)
    }

    fn push(&self, _: PendingEvent) -> Result<(), EventError> {
        Err(EventError::QueueFull)
    }

    fn pop(&self) -> Option<PendingEvent> {
        None
    }

    fn subscription(&self, _: usize) -> Option<Subscription> {
        None
    }
}

pub(crate) struct EventTable<S, P>
where
    S: VecStorage<Subscription>,
    P: VecStorage<PendingEvent>,
{
    subscriptions: RefCell<S>,
    pending: RefCell<P>,
}

impl<S, P> Default for EventTable<S, P>
where
    S: VecStorage<Subscription> + Default,
    P: VecStorage<PendingEvent> + Default,
{
    fn default() -> Self {
        Self {
            subscriptions: RefCell::new(S::default()),
            pending: RefCell::new(P::default()),
        }
    }
}

impl<S, P> EventStore for EventTable<S, P>
where
    S: VecStorage<Subscription>,
    P: VecStorage<PendingEvent>,
{
    fn subscribe(&self, subscription: Subscription) -> Result<(), EventError> {
        self.subscriptions
            .borrow_mut()
            .try_push(subscription)
            .map_err(|error| error.or(EventError::SubscriptionsFull, EventError::AllocationFailed))
    }

    fn push(&self, event: PendingEvent) -> Result<(), EventError> {
        self.pending
            .borrow_mut()
            .try_push(event)
            .map_err(|error| error.or(EventError::QueueFull, EventError::AllocationFailed))
    }

    fn pop(&self) -> Option<PendingEvent> {
        let mut pending = self.pending.borrow_mut();
        let first = *pending.first()?;

        pending.copy_within(1.., 0);
        let len = pending.len();
        pending.truncate(len - 1);

        Some(first)
    }

    fn subscription(&self, index: usize) -> Option<Subscription> {
        self.subscriptions.borrow().get(index).copied()
    }
}

/// queues `event` from `emitter`
pub(crate) fn emit<E>(
    events: &dyn EventStore,
    emitter: EntityId,
    event: E,
) -> Result<(), EventError>
where
    E: Copy + 'static,
{
    const {
        assert!(
            mem::size_of::<E>() <= EVENT_BYTES,
            "an event is at most 16 bytes; send an index or an id instead of larger data"
        );
        assert!(mem::align_of::<E>() <= EVENT_ALIGN);
    }

    let mut payload = EventPayload([MaybeUninit::uninit(); EVENT_BYTES]);
    // SAFETY: the payload is large and aligned enough for `E`, checked above
    unsafe { payload.0.as_mut_ptr().cast::<E>().write(event) };

    events.push(PendingEvent {
        emitter,
        event_type: TypeId::of::<E>(),
        payload,
    })
}

/// subscribes `subscriber` to the `E` events of `emitter`
pub(crate) fn subscribe<T, U, E>(
    events: &dyn EventStore,
    subscriber: Entity<T>,
    emitter: Entity<U>,
    handler: Handler<T, U, E>,
) -> Result<(), EventError>
where
    T: 'static,
    U: 'static,
    E: 'static,
{
    events.subscribe(Subscription {
        subscriber: subscriber.entity_id(),
        emitter: emitter.entity_id(),
        event_type: TypeId::of::<E>(),
        handler: handler as *const (),
        invoke: invoke_handler::<T, U, E>,
    })
}

/// hands every queued event to its subscribers, in the order they were emitted and
/// subscribed. Events their handlers emit are delivered too. Subscribers that are gone
/// are skipped.
///
/// # Panics
///
/// when a subscriber is borrowed, which only happens when this runs inside another
/// runtime call
pub(crate) fn deliver(events: &dyn EventStore, runtime: RuntimeCx<'_>) {
    while let Some(event) = events.pop() {
        let mut index = 0;

        while let Some(subscription) = events.subscription(index) {
            index += 1;

            if subscription.emitter != event.emitter || subscription.event_type != event.event_type
            {
                continue;
            }

            // SAFETY: the subscription's event type matches the payload's, and its
            // trampoline was built for the handler it erased
            let result = unsafe {
                (subscription.invoke)(
                    subscription.handler,
                    subscription.subscriber,
                    event.emitter,
                    event.payload.0.as_ptr().cast(),
                    runtime,
                )
            };

            match result {
                Ok(()) | Err(EntityAccessError::InvalidEntity) => {}
                Err(error) => panic!("could not deliver an event: {error:?}"),
            }
        }
    }
}

unsafe fn invoke_handler<T, U, E>(
    handler: *const (),
    subscriber: EntityId,
    emitter: EntityId,
    event: *const u8,
    runtime: RuntimeCx<'_>,
) -> Result<(), EntityAccessError>
where
    T: 'static,
    U: 'static,
    E: 'static,
{
    // SAFETY: `handler` was cast from a `Handler<T, U, E>` in `subscribe`, which
    // registered this trampoline with the same types
    let handler = unsafe { mem::transmute::<*const (), Handler<T, U, E>>(handler) };
    // SAFETY: the payload holds an `E`, matched by its type id
    let event = unsafe { &*event.cast::<E>() };

    let subscriber = Entity::<T>::from_id(subscriber);
    let emitter = Entity::<U>::from_id(emitter);
    let cx = Context::new_in(subscriber, runtime);

    subscriber.try_update(&cx, |state, cx| handler(state, emitter, event, cx))
}
