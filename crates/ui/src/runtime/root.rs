use crate::{
    Element, Entity, EntityId, EntityRenderFn, MountCx, MountError, NodeId, Render, render_entity,
};

#[derive(Clone, Copy)]
pub(super) struct RuntimeRoot {
    entity: EntityId,
    render: EntityRenderFn,
}

impl RuntimeRoot {
    pub(super) fn from_entity<T>(entity: Entity<T>) -> Self
    where
        T: Render,
    {
        Self {
            entity: entity.entity_id(),
            render: render_entity::<T>,
        }
    }
}

impl Element for RuntimeRoot {
    fn mount(self, cx: &mut MountCx<'_>) -> Result<NodeId, MountError> {
        cx.push_entity(self.entity, self.render)
    }
}
