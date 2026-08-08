use core::cell::Cell;

use crate::{
    Entity, EntityAllocError, EntityId, Listener,
    entity_store::{EntityStore, create_entity},
    listener_store::{ListenerAllocError, ListenerStore, register_listener},
};

pub struct Context<'a, T> {
    pub(crate) entity: Entity<T>,
    pub(crate) store: &'a dyn EntityStore,
    pub(crate) listeners: &'a dyn ListenerStore,
    pub(crate) notified: &'a Cell<bool>,
}

impl<'a, T> Context<'a, T> {
    pub(crate) fn from_parts(
        entity: Entity<T>,
        store: &'a dyn EntityStore,
        listeners: &'a dyn ListenerStore,
        notified: &'a Cell<bool>,
    ) -> Self {
        Self {
            entity,
            store,
            listeners,
            notified,
        }
    }
}

impl<T> Context<'_, T> {
    pub fn entity(&self) -> Entity<T> {
        self.entity
    }

    pub fn entity_id(&self) -> EntityId {
        self.entity.entity_id()
    }

    pub fn notify(&mut self) {
        self.notified.set(true);
    }

    #[allow(clippy::new_ret_no_self)]
    pub fn new<U>(
        &mut self,
        build: impl FnOnce(&mut Context<'_, U>) -> U,
    ) -> Result<Entity<U>, EntityAllocError>
    where
        U: 'static,
    {
        create_entity(self.store, self.listeners, self.notified, build)
    }
}

impl<T: 'static> Context<'_, T> {
    pub fn try_listener<E, F>(&mut self, callback: F) -> Result<Listener<E>, ListenerAllocError>
    where
        E: 'static,
        F: Fn(&mut T, &E, &mut Context<'_, T>) + 'static,
    {
        register_listener(self.listeners, self.entity, callback)
    }

    pub fn listener<E, F>(&mut self, callback: F) -> Listener<E>
    where
        E: 'static,
        F: Fn(&mut T, &E, &mut Context<'_, T>) + 'static,
    {
        match self.try_listener(callback) {
            Ok(listener) => listener,
            Err(_) => panic!("listener arena capacity exceeded"),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{EntityAccessError, EntityArena, listener_store::ListenerArena};

    use super::*;

    struct Root;

    struct Counter {
        value: i32,
    }

    #[test]
    fn context_can_create_entities() {
        let arena = EntityArena::<1024, 16>::default();
        let listeners = ListenerArena::<1024, 16>::default();
        let root = arena.insert(Root).unwrap();
        let notified = Cell::new(false);

        let mut cx = Context::from_parts(root, &arena, &listeners, &notified);

        let counter = cx.new(|_| Counter { value: 42 }).unwrap();

        assert_eq!(counter.read(&cx, |counter| counter.value,), Ok(42));
    }

    #[test]
    fn entity_can_update_through_context() {
        let arena = EntityArena::<1024, 16>::default();
        let listeners = ListenerArena::<1024, 16>::default();
        let root = arena.insert(Root).unwrap();
        let notified = Cell::new(false);

        let mut cx = Context::from_parts(root, &arena, &listeners, &notified);

        let counter = cx.new(|_| Counter { value: 1 }).unwrap();

        counter
            .update(&cx, |counter, cx| {
                counter.value += 1;
                cx.notify();
            })
            .unwrap();

        assert_eq!(counter.read(&cx, |counter| counter.value,), Ok(2));
        assert!(notified.get());
    }

    #[test]
    fn entity_constructors_can_create_entities() {
        struct Child {
            value: u32,
        }

        struct Parent {
            child: Entity<Child>,
        }

        let arena = EntityArena::<1024, 16>::default();
        let listeners = ListenerArena::<1024, 16>::default();
        let root = arena.insert(Root).unwrap();
        let notified = Cell::new(false);

        let mut cx = Context::from_parts(root, &arena, &listeners, &notified);

        let parent = cx
            .new(|cx| {
                let child = cx.new(|_| Child { value: 123 }).unwrap();
                Parent { child }
            })
            .unwrap();

        let child = parent.read(&cx, |parent| parent.child).unwrap();

        assert_eq!(child.read(&cx, |child| child.value,), Ok(123));
    }

    #[test]
    fn self_identity_during_construction() {
        struct Parent {
            child: Entity<Child>,
        }

        struct Child {
            parent: Entity<Parent>,
        }

        let arena = EntityArena::<1024, 16>::default();
        let listeners = ListenerArena::<1024, 16>::default();
        let root = arena.insert(Root).unwrap();
        let notified = Cell::new(false);

        let mut cx = Context::from_parts(root, &arena, &listeners, &notified);

        let parent = cx
            .new(|cx| {
                let me = cx.entity();

                let child = cx.new(|_| Child { parent: me }).unwrap();

                Parent { child }
            })
            .unwrap();

        let child = parent.read(&cx, |p| p.child).unwrap();

        let referenced_parent = child.read(&cx, |c| c.parent).unwrap();

        assert_eq!(referenced_parent, parent);
    }

    #[test]
    fn initializing_entity_cannot_be_read() {
        let arena = EntityArena::<1024, 16>::default();
        let listeners = ListenerArena::<1024, 16>::default();
        let root = arena.insert(Root).unwrap();
        let notified = Cell::new(false);

        let mut cx = Context::from_parts(root, &arena, &listeners, &notified);

        let _ = cx
            .new(|cx| {
                let me = cx.entity();

                assert!(matches!(
                    me.read(cx, |_| ()),
                    Err(EntityAccessError::NotReady),
                ));

                Counter { value: 0 }
            })
            .unwrap();
    }
}
