use core::{any::TypeId, cell::Cell};

use super::dependencies::{Dependencies, Source};
use crate::{
    EntityId, ReleaseError,
    callback::CallbackArena,
    emitter::EventTable,
    entity::{EntityArena, LeaseTable, ReleaseQueue, no_release_room},
    global::GlobalArena,
};

/// the runtime tables that rendering, listeners and entity updates reach through a
/// [`Context`](crate::Context)
#[derive(Clone, Copy)]
pub struct RuntimeCx<'a> {
    pub(crate) entities: &'a EntityArena,
    pub(crate) globals: &'a GlobalArena,
    pub(crate) callbacks: &'a CallbackArena,
    /// set when every entity needs to render again, such as after the root changed
    pub(crate) all_dirty: &'a Cell<bool>,
    /// what each entity read while rendering
    pub(crate) dependencies: Option<&'a Dependencies>,
    /// subscriptions, and the events waiting for delivery
    pub(crate) events: Option<&'a EventTable>,
    /// the entities borrowed by the references contexts handed out
    pub(crate) leases: Option<&'a LeaseTable>,
    /// the entities released and waiting to be dropped
    pub(crate) releases: Option<&'a ReleaseQueue>,
    /// the entity being rendered, which owns the callbacks registered meanwhile
    pub(crate) rendering: Option<EntityId>,
}

impl<'a> RuntimeCx<'a> {
    pub(crate) fn new(
        entities: &'a EntityArena,
        globals: &'a GlobalArena,
        callbacks: &'a CallbackArena,
        all_dirty: &'a Cell<bool>,
    ) -> Self {
        Self {
            entities,
            globals,
            callbacks,
            all_dirty,
            dependencies: None,
            events: None,
            leases: None,
            releases: None,
            rendering: None,
        }
    }

    pub(crate) fn with_events(self, events: &'a EventTable) -> Self {
        Self {
            events: Some(events),
            ..self
        }
    }

    pub(crate) fn with_leases(self, leases: &'a LeaseTable) -> Self {
        Self {
            leases: Some(leases),
            ..self
        }
    }

    pub(crate) fn with_releases(self, releases: &'a ReleaseQueue) -> Self {
        Self {
            releases: Some(releases),
            ..self
        }
    }

    /// queues `entity` to be dropped once nothing borrows it and it isn't mounted
    pub(crate) fn release(self, entity: EntityId) -> Result<(), ReleaseError> {
        if !self.entities.contains(entity) {
            return Err(ReleaseError::InvalidEntity);
        }

        let Some(releases) = self.releases else {
            return Err(no_release_room());
        };
        releases.push(entity)
    }

    pub(crate) fn with_dependencies(self, dependencies: &'a Dependencies) -> Self {
        Self {
            dependencies: Some(dependencies),
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
        if let (Some(reader), Some(dependencies)) = (self.rendering, self.dependencies) {
            dependencies.record(reader, source);
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
        let Some(dependencies) = self.dependencies else {
            return;
        };
        if dependencies.is_incomplete() {
            self.all_dirty.set(true);
            return;
        }

        dependencies.for_each_reader(source, |reader| {
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
