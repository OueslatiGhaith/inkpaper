use core::{any::TypeId, cell::Cell};

use crate::{
    Context, Element, Entity, EntityBorrowKind, EntityId, FrameStore, IntoElement, MountCx,
    MountError, NodeId,
    entity_store::{EntityStore, RawEntityBorrow},
    listener_store::ListenerStore,
};

pub trait Render: Sized + 'static {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a;
}

pub(crate) type EntityRenderFn = fn(
    entity: EntityId,
    entities: &dyn EntityStore,
    listeners: &dyn ListenerStore,
    notified: &Cell<bool>,
    frame: &mut dyn FrameStore,
) -> Result<NodeId, MountError>;

pub(crate) fn render_entity<T>(
    entity_id: EntityId,
    entities: &dyn EntityStore,
    listeners: &dyn ListenerStore,
    notified: &Cell<bool>,
    frame: &mut dyn FrameStore,
) -> Result<NodeId, MountError>
where
    T: Render,
{
    let borrow = RawEntityBorrow::acquire(
        entities,
        entity_id,
        TypeId::of::<T>(),
        EntityBorrowKind::Exclusive,
    )?;

    let state = unsafe { &mut *borrow.ptr().cast::<T>().as_ptr() };
    let entity = Entity::<T>::from_id(entity_id);
    let mut cx = Context::from_parts(entity, entities, listeners, notified);
    let element = state.render(&mut cx).into_element();
    let mut mount_cx = MountCx::new(frame);
    let root = element.mount(&mut mount_cx)?;

    Ok(root)
}
