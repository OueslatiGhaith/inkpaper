use core::any::TypeId;

use crate::{
    BorrowKind, Canvas, Entity, EntityAllocError, EntityId, EventEmitter, Listener, OutOfMemory,
    PaintCx, RuntimeCx,
    callback::{CallbackAllocError, register_canvas_callback, register_listener},
    emitter::{self, Handler},
    entity::{LeaseOwner, create_entity},
    global::{Global, GlobalAccessError, GlobalMut, GlobalRef, GlobalStore},
    runtime::{DependencyStore, Source},
};

pub struct Context<'a, T> {
    pub(crate) entity: Entity<T>,
    pub(crate) runtime: RuntimeCx<'a>,
    /// the leases of the references [`Entity::read`] handed out through this context.
    /// They are released when it drops
    pub(crate) lease_owner: LeaseOwner,
}

impl<'a, T> Context<'a, T> {
    pub(crate) fn new_in(entity: Entity<T>, runtime: RuntimeCx<'a>) -> Self {
        Self {
            entity,
            runtime,
            lease_owner: runtime.leases.next_owner(),
        }
    }

    pub fn entity(&self) -> Entity<T> {
        self.entity
    }

    pub fn entity_id(&self) -> EntityId {
        self.entity.entity_id()
    }

    /// marks this entity as changed, so it renders again
    pub fn notify(&self) {
        self.runtime.notify(self.entity.entity_id());
    }

    /// creates an entity from the state `build` returns. `build` gets the new entity's
    /// own context, so it can create listeners and child entities.
    ///
    /// # Panics
    ///
    /// when every entity id is taken or the pool has no room. See [`Self::try_new`]
    #[allow(clippy::new_ret_no_self)]
    #[track_caller]
    pub fn new<U>(&self, build: impl FnOnce(&mut Context<'_, U>) -> U) -> Entity<U>
    where
        U: 'static,
    {
        match self.try_new(build) {
            Ok(entity) => entity,
            Err(error) => panic!("could not allocate entity: {error:?}"),
        }
    }

    pub fn try_new<U>(
        &self,
        build: impl FnOnce(&mut Context<'_, U>) -> U,
    ) -> Result<Entity<U>, EntityAllocError>
    where
        U: 'static,
    {
        create_entity(self.runtime, build)
    }

    pub fn try_global<G>(&self) -> Result<GlobalRef<'a, G>, GlobalAccessError>
    where
        G: Global,
    {
        // a render that finds the global missing renders again once it is set
        self.runtime.record_read(Source::Global(TypeId::of::<G>()));

        GlobalRef::acquire(self.runtime.globals)
    }

    pub fn global<G>(&self) -> GlobalRef<'a, G>
    where
        G: Global,
    {
        self.try_global::<G>()
            .unwrap_or_else(|_| panic!("requested global is not available"))
    }

    pub fn try_global_mut<G>(&self) -> Result<GlobalMut<'a, G>, GlobalAccessError>
    where
        G: Global,
    {
        GlobalMut::acquire(self.runtime)
    }

    pub fn global_mut<G>(&self) -> GlobalMut<'a, G>
    where
        G: Global,
    {
        self.try_global_mut::<G>()
            .unwrap_or_else(|_| panic!("requested global is not available for mutation"))
    }
}

impl<T: 'static> Context<'_, T> {
    pub fn try_listener<E, F>(&self, callback: F) -> Result<Listener<E>, CallbackAllocError>
    where
        E: 'static,
        F: Fn(&mut T, &E, &mut Context<'_, T>) + 'static,
    {
        let owner = self.runtime.callback_owner(self.entity_id());

        register_listener(self.runtime.callbacks, owner, self.entity, callback)
    }

    pub fn listener<E, F>(&self, callback: F) -> Listener<E>
    where
        E: 'static,
        F: Fn(&mut T, &E, &mut Context<'_, T>) + 'static,
    {
        self.try_listener(callback)
            .unwrap_or_else(|error| panic!("could not allocate listener: {error:?}"))
    }

    pub fn try_canvas<F>(&self, callback: F) -> Result<Canvas, CallbackAllocError>
    where
        F: Fn(&T, &mut PaintCx<'_>) + 'static,
    {
        // the canvas paints from its target's state
        self.runtime.record_read(Source::Entity(self.entity_id()));

        let owner = self.runtime.callback_owner(self.entity_id());
        let callback =
            register_canvas_callback(self.runtime.callbacks, owner, self.entity, callback)?;

        Ok(Canvas::from_entity_callback(callback))
    }

    pub fn canvas<F>(&self, callback: F) -> Canvas
    where
        F: Fn(&T, &mut PaintCx<'_>) + 'static,
    {
        self.try_canvas(callback)
            .unwrap_or_else(|error| panic!("could not allocate canvas callback: {error:?}"))
    }

    /// tells this entity's subscribers that `event` happened. They get it once the
    /// runtime call running now returns, such as the listener of a tap, so they can
    /// update this entity.
    ///
    /// # Panics
    ///
    /// when the pool has no room for the event. See [`Self::try_emit`]
    #[track_caller]
    pub fn emit<E>(&self, event: E)
    where
        T: EventEmitter<E>,
        E: Copy + 'static,
    {
        if let Err(error) = self.try_emit(event) {
            panic!("could not emit event: {error:?}");
        }
    }

    pub fn try_emit<E>(&self, event: E) -> Result<(), OutOfMemory>
    where
        T: EventEmitter<E>,
        E: Copy + 'static,
    {
        emitter::emit(self.runtime.events, self.entity_id(), event)
    }

    /// calls `handler` with this entity, each `E` event `emitter` emits, and its own
    /// context. Entities live as long as the runtime, and so do subscriptions.
    ///
    /// # Panics
    ///
    /// when the pool has no room for the subscription. See [`Self::try_subscribe`]
    #[track_caller]
    pub fn subscribe<U, E>(&self, emitter: Entity<U>, handler: Handler<T, U, E>)
    where
        U: EventEmitter<E>,
        E: 'static,
    {
        if let Err(error) = self.try_subscribe(emitter, handler) {
            panic!("could not subscribe: {error:?}");
        }
    }

    pub fn try_subscribe<U, E>(
        &self,
        emitter: Entity<U>,
        handler: Handler<T, U, E>,
    ) -> Result<(), OutOfMemory>
    where
        U: EventEmitter<E>,
        E: 'static,
    {
        emitter::subscribe(self.runtime.events, self.entity, emitter, handler)
    }
}

impl<T> Drop for Context<'_, T> {
    fn drop(&mut self) {
        let entities = self.runtime.entities;

        self.runtime
            .leases
            .release_owner(self.lease_owner, &mut |entity| {
                entities.release(entity, BorrowKind::Shared);
            });
    }
}

#[derive(Clone, Copy)]
pub struct AppContext<'a> {
    globals: &'a dyn GlobalStore,
    /// records the global reads of the entity being rendered
    dependencies: &'a dyn DependencyStore,
    rendering: Option<EntityId>,
}

impl<'a> AppContext<'a> {
    #[cfg(test)]
    pub(crate) const fn from_globals(globals: &'a dyn GlobalStore) -> Self {
        Self {
            globals,
            dependencies: &crate::runtime::NoDependencies,
            rendering: None,
        }
    }

    pub(crate) fn from_runtime(runtime: RuntimeCx<'a>) -> Self {
        Self {
            globals: runtime.globals,
            dependencies: runtime.dependencies,
            rendering: runtime.rendering,
        }
    }

    pub fn try_global<G>(&self) -> Result<GlobalRef<'a, G>, GlobalAccessError>
    where
        G: Global,
    {
        if let Some(reader) = self.rendering {
            self.dependencies
                .record(reader, Source::Global(TypeId::of::<G>()));
        }

        GlobalRef::acquire(self.globals)
    }

    pub fn global<G>(&self) -> GlobalRef<'a, G>
    where
        G: Global,
    {
        self.try_global::<G>()
            .unwrap_or_else(|_| panic!("requested global is not available"))
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        CallbackArena, EntityAccessError, EntityArena, GlobalArena, pool::testing::pool_ref,
    };

    use core::cell::Cell;

    use super::*;
    use crate::entity::EntityStore;

    struct Root;

    struct Counter {
        value: i32,
    }

    #[test]
    fn entity_can_update_through_context() {
        let arena = EntityArena::new_in(pool_ref());
        let callbacks = CallbackArena::new_in(pool_ref());
        let globals = GlobalArena::new_in(pool_ref());
        let root = arena.insert(Root).unwrap();
        let all_dirty = Cell::new(false);

        let cx = Context::new_in(
            root,
            RuntimeCx::new(&arena, &globals, &callbacks, &all_dirty),
        );

        let counter = cx.new(|_| Counter { value: 1 });

        counter.update(&cx, |counter, cx| {
            counter.value += 1;
            cx.notify();
        });

        assert_eq!(counter.read_with(&cx, |counter| counter.value), 2);
        assert!(arena.is_dirty(counter.entity_id()));
        assert!(!arena.is_dirty(root.entity_id()));
    }

    #[test]
    fn entity_notify_works_while_the_entity_is_borrowed() {
        let arena = EntityArena::new_in(pool_ref());
        let callbacks = CallbackArena::new_in(pool_ref());
        let globals = GlobalArena::new_in(pool_ref());
        let root = arena.insert(Root).unwrap();
        let all_dirty = Cell::new(false);

        let cx = Context::new_in(
            root,
            RuntimeCx::new(&arena, &globals, &callbacks, &all_dirty),
        );

        let counter = cx.new(|_| Counter { value: 0 });

        counter.update(&cx, |_, cx| counter.notify(cx));

        assert!(arena.is_dirty(counter.entity_id()));
        assert!(!arena.is_dirty(root.entity_id()));
    }

    #[test]
    #[should_panic(expected = "could not read entity: NotReady")]
    fn reading_an_initializing_entity_panics() {
        let arena = EntityArena::new_in(pool_ref());
        let callbacks = CallbackArena::new_in(pool_ref());
        let globals = GlobalArena::new_in(pool_ref());
        let root = arena.insert(Root).unwrap();
        let all_dirty = Cell::new(false);

        let cx = Context::new_in(
            root,
            RuntimeCx::new(&arena, &globals, &callbacks, &all_dirty),
        );

        cx.new(|cx| {
            cx.entity().read_with(cx, |_| ());

            Counter { value: 0 }
        });
    }

    #[test]
    fn entity_constructors_can_create_entities() {
        struct Child {
            value: u32,
        }

        struct Parent {
            child: Entity<Child>,
        }

        let arena = EntityArena::new_in(pool_ref());
        let callbacks = CallbackArena::new_in(pool_ref());
        let globals = GlobalArena::new_in(pool_ref());
        let root = arena.insert(Root).unwrap();
        let all_dirty = Cell::new(false);

        let cx = Context::new_in(
            root,
            RuntimeCx::new(&arena, &globals, &callbacks, &all_dirty),
        );

        let parent = cx.new(|cx| {
            let child = cx.new(|_| Child { value: 123 });
            Parent { child }
        });

        let child = parent.read_with(&cx, |parent| parent.child);

        assert_eq!(child.read_with(&cx, |child| child.value), 123);
    }

    #[test]
    fn self_identity_during_construction() {
        struct Parent {
            child: Entity<Child>,
        }

        struct Child {
            parent: Entity<Parent>,
        }

        let arena = EntityArena::new_in(pool_ref());
        let callbacks = CallbackArena::new_in(pool_ref());
        let globals = GlobalArena::new_in(pool_ref());
        let root = arena.insert(Root).unwrap();
        let all_dirty = Cell::new(false);

        let cx = Context::new_in(
            root,
            RuntimeCx::new(&arena, &globals, &callbacks, &all_dirty),
        );

        let parent = cx.new(|cx| {
            let me = cx.entity();

            let child = cx.new(|_| Child { parent: me });

            Parent { child }
        });

        let child = parent.read_with(&cx, |p| p.child);

        let referenced_parent = child.read_with(&cx, |c| c.parent);

        assert_eq!(referenced_parent, parent);
    }

    #[test]
    fn initializing_entity_cannot_be_read() {
        let arena = EntityArena::new_in(pool_ref());
        let callbacks = CallbackArena::new_in(pool_ref());
        let globals = GlobalArena::new_in(pool_ref());
        let root = arena.insert(Root).unwrap();
        let all_dirty = Cell::new(false);

        let cx = Context::new_in(
            root,
            RuntimeCx::new(&arena, &globals, &callbacks, &all_dirty),
        );

        cx.new(|cx| {
            let me = cx.entity();

            assert!(matches!(
                me.try_read_with(cx, |_| ()),
                Err(EntityAccessError::NotReady),
            ));

            Counter { value: 0 }
        });
    }
}
