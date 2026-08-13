use core::cell::Cell;

use crate::{
    Canvas, CanvasPainter, Entity, EntityAllocError, EntityId, Listener, Rect,
    callback_store::{
        CallbackAllocError, CallbackStore, register_canvas_callback, register_listener,
    },
    entity_store::{EntityStore, create_entity},
};

pub struct Context<'a, T> {
    pub(crate) entity: Entity<T>,
    pub(crate) store: &'a dyn EntityStore,
    pub(crate) callbacks: &'a dyn CallbackStore,
    pub(crate) notified: &'a Cell<bool>,
}

impl<'a, T> Context<'a, T> {
    pub(crate) fn from_parts(
        entity: Entity<T>,
        store: &'a dyn EntityStore,
        callbacks: &'a dyn CallbackStore,
        notified: &'a Cell<bool>,
    ) -> Self {
        Self {
            entity,
            store,
            callbacks,
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
        create_entity(self.store, self.callbacks, self.notified, build)
    }
}

impl<T: 'static> Context<'_, T> {
    pub fn try_listener<E, F>(&mut self, callback: F) -> Result<Listener<E>, CallbackAllocError>
    where
        E: 'static,
        F: Fn(&mut T, &E, &mut Context<'_, T>) + 'static,
    {
        register_listener(self.callbacks, self.entity, callback)
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

    pub fn try_canvas<F>(&mut self, callback: F) -> Result<Canvas, CallbackAllocError>
    where
        F: Fn(&T, Rect, &mut dyn CanvasPainter) + 'static,
    {
        let callback = register_canvas_callback(self.callbacks, self.entity, callback)?;

        Ok(Canvas::from_entity_callback(callback))
    }

    pub fn canvas<F>(&mut self, callback: F) -> Canvas
    where
        F: Fn(&T, Rect, &mut dyn CanvasPainter) + 'static,
    {
        match self.try_canvas(callback) {
            Ok(canvas) => canvas,
            Err(_) => panic!("callback arena capacity exceeded"),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{EntityAccessError, EntityArena, callback_store::CallbackArena};

    use super::*;

    struct Root;

    struct Counter {
        value: i32,
    }

    #[test]
    fn context_can_create_entities() {
        let arena = EntityArena::<1024, 16>::default();
        let callbacks = CallbackArena::<1024, 16>::default();
        let root = arena.insert(Root).unwrap();
        let notified = Cell::new(false);

        let mut cx = Context::from_parts(root, &arena, &callbacks, &notified);

        let counter = cx.new(|_| Counter { value: 42 }).unwrap();

        assert_eq!(counter.read(&cx, |counter| counter.value,), Ok(42));
    }

    #[test]
    fn entity_can_update_through_context() {
        let arena = EntityArena::<1024, 16>::default();
        let callbacks = CallbackArena::<1024, 16>::default();
        let root = arena.insert(Root).unwrap();
        let notified = Cell::new(false);

        let mut cx = Context::from_parts(root, &arena, &callbacks, &notified);

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
        let callbacks = CallbackArena::<1024, 16>::default();
        let root = arena.insert(Root).unwrap();
        let notified = Cell::new(false);

        let mut cx = Context::from_parts(root, &arena, &callbacks, &notified);

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
        let callbacks = CallbackArena::<1024, 16>::default();
        let root = arena.insert(Root).unwrap();
        let notified = Cell::new(false);

        let mut cx = Context::from_parts(root, &arena, &callbacks, &notified);

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
        let callbacks = CallbackArena::<1024, 16>::default();
        let root = arena.insert(Root).unwrap();
        let notified = Cell::new(false);

        let mut cx = Context::from_parts(root, &arena, &callbacks, &notified);

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
