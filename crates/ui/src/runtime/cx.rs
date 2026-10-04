use core::cell::Cell;

use crate::{callback::CallbackStore, entity::EntityStore, global::GlobalStore};

/// the runtime tables that rendering, listeners and entity updates reach through a
/// [`Context`](crate::Context)
#[derive(Clone, Copy)]
pub struct RuntimeCx<'a> {
    pub(crate) entities: &'a dyn EntityStore,
    pub(crate) globals: &'a dyn GlobalStore,
    pub(crate) callbacks: &'a dyn CallbackStore,
    /// set when every entity needs to render again, such as after a global changed
    pub(crate) all_dirty: &'a Cell<bool>,
}

impl<'a> RuntimeCx<'a> {
    pub(crate) fn new(
        entities: &'a dyn EntityStore,
        globals: &'a dyn GlobalStore,
        callbacks: &'a dyn CallbackStore,
        all_dirty: &'a Cell<bool>,
    ) -> Self {
        Self {
            entities,
            globals,
            callbacks,
            all_dirty,
        }
    }

    /// whether any entity needs to render again
    pub(crate) fn is_dirty(self) -> bool {
        self.all_dirty.get() || self.entities.has_dirty()
    }

    pub(crate) fn clear_dirty(self) {
        self.all_dirty.set(false);
        self.entities.clear_dirty();
    }
}
