use core::{any::TypeId, cell::Cell};

use super::dependencies::{DependencyStore, NoDependencies, Source};
use crate::{EntityId, callback::CallbackStore, entity::EntityStore, global::GlobalStore};

/// the runtime tables that rendering, listeners and entity updates reach through a
/// [`Context`](crate::Context)
#[derive(Clone, Copy)]
pub struct RuntimeCx<'a> {
    pub(crate) entities: &'a dyn EntityStore,
    pub(crate) globals: &'a dyn GlobalStore,
    pub(crate) callbacks: &'a dyn CallbackStore,
    /// set when every entity needs to render again, such as after the root changed
    pub(crate) all_dirty: &'a Cell<bool>,
    /// what each entity read while rendering
    pub(crate) dependencies: &'a dyn DependencyStore,
    /// the entity being rendered, which owns the callbacks registered meanwhile
    pub(crate) rendering: Option<EntityId>,
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
            dependencies: &NoDependencies,
            rendering: None,
        }
    }

    pub(crate) fn with_dependencies(self, dependencies: &'a dyn DependencyStore) -> Self {
        Self {
            dependencies,
            ..self
        }
    }

    pub(crate) fn rendering(self, entity: EntityId) -> Self {
        Self {
            rendering: Some(entity),
            ..self
        }
    }

    /// the owner of a callback registered now on `target`: the entity being rendered,
    /// or the target itself outside rendering
    pub(crate) fn callback_owner(self, target: EntityId) -> EntityId {
        self.rendering.unwrap_or(target)
    }

    /// records that the entity being rendered, if any, read `source`
    pub(crate) fn record_read(self, source: Source) {
        if let Some(reader) = self.rendering {
            self.dependencies.record(reader, source);
        }
    }

    /// marks `entity` and the entities that read it as needing to render again
    pub(crate) fn notify(self, entity: EntityId) {
        self.entities.mark_dirty(entity);
        self.mark_readers(Source::Entity(entity));
    }

    /// marks the entities that read the global as needing to render again
    pub(crate) fn global_changed(self, global: TypeId) {
        self.mark_readers(Source::Global(global));
    }

    fn mark_readers(self, source: Source) {
        if self.dependencies.is_incomplete() {
            self.all_dirty.set(true);
            return;
        }

        self.dependencies.for_each_reader(source, &mut |reader| {
            // a render that changes what it read already sees the change
            if Some(reader) != self.rendering {
                self.entities.mark_dirty(reader);
            }
        });
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
