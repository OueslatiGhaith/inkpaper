use crate::{Entity, EntityId};

pub struct Context<'a, T> {
    entity: Entity<T>,
    notified: &'a mut bool,
}

impl<'a, T> Context<'a, T> {
    pub(crate) fn new(entity: Entity<T>, notified: &'a mut bool) -> Self {
        Self { entity, notified }
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
        *self.notified = true;
    }
}
