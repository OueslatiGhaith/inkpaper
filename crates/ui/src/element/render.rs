use core::any::TypeId;

use crate::{
    AppContext, BorrowKind, Context, Element, Entity, EntityId, FrameStore, IntoElement, MountCx,
    MountError, NodeId, RuntimeCx, entity::RawEntityBorrow,
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
    fn render(self, cx: &AppContext<'_>) -> impl IntoElement;
}

impl<T> Element for T
where
    T: RenderOnce,
{
    fn mount(self, cx: &mut MountCx<'_>) -> Result<NodeId, MountError> {
        let app = cx.app_context();
        self.render(&app).into_element().mount(cx)
    }
}

pub(crate) type EntityRenderFn = fn(
    entity: EntityId,
    runtime: RuntimeCx<'_>,
    frame: &mut dyn FrameStore,
) -> Result<NodeId, MountError>;

pub(crate) fn render_entity<T>(
    entity_id: EntityId,
    runtime: RuntimeCx<'_>,
    frame: &mut dyn FrameStore,
) -> Result<NodeId, MountError>
where
    T: Render,
{
    let borrow = RawEntityBorrow::acquire(
        runtime.entities,
        entity_id,
        TypeId::of::<T>(),
        BorrowKind::Exclusive,
    )?;

    let state = unsafe { &mut *borrow.ptr().cast::<T>().as_ptr() };
    let entity = Entity::<T>::from_id(entity_id);
    let runtime = runtime.rendering(entity_id);
    let mut cx = Context::new_in(entity, runtime);
    let element = state.render(&mut cx).into_element();
    let app = AppContext::from_runtime(runtime);
    let mut mount_cx = MountCx::new(frame, app);

    element.mount(&mut mount_cx)
}
