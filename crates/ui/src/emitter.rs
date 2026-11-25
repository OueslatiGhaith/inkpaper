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

use crate::{
    Context, Entity, EntityAccessError, EntityId, RuntimeCx,
    pool::{OutOfMemory, PoolRef, PoolVec, Table},
};

/// marks an entity type as emitting events of type `E`, for [`Context::emit`] and
/// [`Context::subscribe`].
///
/// events are copied into the runtime's queue, so they are small `Copy` values of at
/// most 16 bytes, such as an enum naming what happened
pub trait EventEmitter<E: 'static>: 'static {}

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

/// the error for a `T` when a context has no event table
fn no_room<T>() -> OutOfMemory {
    OutOfMemory {
        table: Table::Events,
        requested: mem::size_of::<T>(),
    }
}

pub(crate) struct EventTable {
    subscriptions: RefCell<PoolVec<Subscription>>,
    pending: RefCell<PoolVec<PendingEvent>>,
}

impl EventTable {
    pub(crate) fn new_in(pool: PoolRef) -> Self {
        Self {
            subscriptions: RefCell::new(PoolVec::new_in(pool, Table::Events)),
            pending: RefCell::new(PoolVec::new_in(pool, Table::Events)),
        }
    }

    /// forgets the subscriptions of a released entity, as subscriber or emitter
    pub(crate) fn forget(&self, entity: EntityId) {
        let mut subscriptions = self.subscriptions.borrow_mut();
        let mut kept = 0;

        for index in 0..subscriptions.len() {
            let subscription = subscriptions[index];

            if subscription.subscriber != entity && subscription.emitter != entity {
                subscriptions[kept] = subscription;
                kept += 1;
            }
        }

        subscriptions.truncate(kept);
    }

    #[cfg(test)]
    pub(crate) fn subscription_count(&self) -> usize {
        self.subscriptions.borrow().len()
    }

    pub(crate) fn subscribe(&self, subscription: Subscription) -> Result<(), OutOfMemory> {
        self.subscriptions.borrow_mut().try_push(subscription)
    }

    pub(crate) fn push(&self, event: PendingEvent) -> Result<(), OutOfMemory> {
        self.pending.borrow_mut().try_push(event)
    }

    /// takes the event emitted first
    pub(crate) fn pop(&self) -> Option<PendingEvent> {
        let mut pending = self.pending.borrow_mut();
        let first = *pending.first()?;

        pending.copy_within(1.., 0);
        let len = pending.len();
        pending.truncate(len - 1);

        Some(first)
    }

    pub(crate) fn subscription(&self, index: usize) -> Option<Subscription> {
        self.subscriptions.borrow().get(index).copied()
    }
}

/// queues `event` from `emitter`
pub(crate) fn emit<E>(
    events: Option<&EventTable>,
    emitter: EntityId,
    event: E,
) -> Result<(), OutOfMemory>
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

    let Some(events) = events else {
        return Err(no_room::<PendingEvent>());
    };

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
    events: Option<&EventTable>,
    subscriber: Entity<T>,
    emitter: Entity<U>,
    handler: Handler<T, U, E>,
) -> Result<(), OutOfMemory>
where
    T: 'static,
    U: 'static,
    E: 'static,
{
    let Some(events) = events else {
        return Err(no_room::<Subscription>());
    };

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
pub(crate) fn deliver(events: &EventTable, runtime: RuntimeCx<'_>) {
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
