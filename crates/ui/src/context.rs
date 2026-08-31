use core::cell::Cell;

use crate::{
    Canvas, CanvasPainter, Entity, EntityAllocError, EntityId, Listener, Rect,
    callback::{
        CallbackAllocError, CallbackStore, register_canvas_callback, register_listener,
    },
    entity::{EntityStore, create_entity},
    global::{Global, GlobalAccessError, GlobalMut, GlobalRef, GlobalStore},
};

pub struct Context<'a, T> {
    pub(crate) entity: Entity<T>,
    pub(crate) store: &'a dyn EntityStore,
    pub(crate) globals: &'a dyn GlobalStore,
    pub(crate) callbacks: &'a dyn CallbackStore,
    pub(crate) notified: &'a Cell<bool>,
}

impl<'a, T> Context<'a, T> {
    pub(crate) fn from_parts(
        entity: Entity<T>,
        store: &'a dyn EntityStore,
        globals: &'a dyn GlobalStore,
        callbacks: &'a dyn CallbackStore,
        notified: &'a Cell<bool>,
    ) -> Self {
        Self {
            entity,
            store,
            globals,
            callbacks,
            notified,
        }
    }

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
        create_entity(
            self.store,
            self.globals,
            self.callbacks,
            self.notified,
            build,
        )
    }

    pub fn try_global<G>(&self) -> Result<GlobalRef<'a, G>, GlobalAccessError>
    where
        G: Global,
    {
        GlobalRef::acquire(self.globals)
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
        GlobalMut::acquire(self.globals, self.notified)
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

#[derive(Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct AppContext<'a> {
    globals: &'a dyn GlobalStore,
}

impl<'a> AppContext<'a> {
    pub(crate) const fn from_globals(globals: &'a dyn GlobalStore) -> Self {
        Self { globals }
    }

    pub fn try_global<G>(&self) -> Result<GlobalRef<'a, G>, GlobalAccessError>
    where
        G: Global,
    {
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
    use crate::{EntityAccessError, EntityArena, GlobalArena, callback::CallbackArena};

    use super::*;

    struct Root;

    struct Counter {
        value: i32,
    }

    #[test]
    fn context_can_create_entities() {
        let arena = EntityArena::<1024, 16>::default();
        let callbacks = CallbackArena::<1024, 16>::default();
        let globals = GlobalArena::<1024, 16>::default();
        let root = arena.insert(Root).unwrap();
        let notified = Cell::new(false);

        let mut cx = Context::from_parts(root, &arena, &globals, &callbacks, &notified);

        let counter = cx.new(|_| Counter { value: 42 }).unwrap();

        assert_eq!(counter.read(&cx, |counter| counter.value,), Ok(42));
    }

    #[test]
    fn entity_can_update_through_context() {
        let arena = EntityArena::<1024, 16>::default();
        let callbacks = CallbackArena::<1024, 16>::default();
        let globals = GlobalArena::<1024, 16>::default();
        let root = arena.insert(Root).unwrap();
        let notified = Cell::new(false);

        let mut cx = Context::from_parts(root, &arena, &globals, &callbacks, &notified);

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
        let globals = GlobalArena::<0, 0>::default();
        let root = arena.insert(Root).unwrap();
        let notified = Cell::new(false);

        let mut cx = Context::from_parts(root, &arena, &globals, &callbacks, &notified);

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
        let globals = GlobalArena::<0, 0>::default();
        let root = arena.insert(Root).unwrap();
        let notified = Cell::new(false);

        let mut cx = Context::from_parts(root, &arena, &globals, &callbacks, &notified);

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
        let globals = GlobalArena::<0, 0>::default();
        let root = arena.insert(Root).unwrap();
        let notified = Cell::new(false);

        let mut cx = Context::from_parts(root, &arena, &globals, &callbacks, &notified);

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
