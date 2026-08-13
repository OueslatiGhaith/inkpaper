use core::{any::TypeId, cell::Cell};

use crate::{
    Context, Element, Entity, EntityBorrowKind, EntityId, FrameStore, IntoElement, MountCx,
    MountError, NodeId,
    callback_store::CallbackStore,
    entity_store::{EntityStore, RawEntityBorrow},
};

/// a persistent component backed by an [`Entity`]
///
/// `Render` components may hold mutable state across frames and receive a [`Context`]
/// for interacting with the runtimes
pub trait Render: Sized + 'static {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a;
}

/// a temporary, stateless component that is rendered exactly once.
///
/// unlike [`Render`], a `RenderOnce` value:
/// - does not require an [`Entity`]
/// - does not occupy persistent runtime storage
/// - may borrow non-`'static` data
/// - is consumed when rendered
///
/// `RenderOnce` is intended for reusable composition
pub trait RenderOnce: Sized {
    fn render(self) -> impl IntoElement;
}

impl<T> Element for T
where
    T: RenderOnce,
{
    fn mount(self, cx: &mut MountCx<'_>) -> Result<NodeId, MountError> {
        self.render().into_element().mount(cx)
    }
}

pub(crate) type EntityRenderFn = fn(
    entity: EntityId,
    entities: &dyn EntityStore,
    callbacks: &dyn CallbackStore,
    notified: &Cell<bool>,
    frame: &mut dyn FrameStore,
) -> Result<NodeId, MountError>;

pub(crate) fn render_entity<T>(
    entity_id: EntityId,
    entities: &dyn EntityStore,
    callbacks: &dyn CallbackStore,
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
    let mut cx = Context::from_parts(entity, entities, callbacks, notified);
    let element = state.render(&mut cx).into_element();
    let mut mount_cx = MountCx::new(frame);
    let root = element.mount(&mut mount_cx)?;

    Ok(root)
}
