use crate::{
    Context, DamageRegion, Entity, EntityAccessError, FrameBuildError, Offset, Point,
    RenderInvalidation, ResourcePainter, Runtime, Size, TextMeasurer,
    callback::ListenerInvokeError,
};

/// application-facing runtime operations.
///
/// this intentionally excludes frame construction, layout, painting, render resources,
/// and backend-specific behavior.
pub trait RuntimeApi {
    fn update<T, R, F>(&self, entity: Entity<T>, update: F) -> Result<R, EntityAccessError>
    where
        T: 'static,
        F: FnOnce(&mut T, &mut Context<'_, T>) -> R;

    fn focus_previous(&mut self) -> bool;

    fn focus_next(&mut self) -> bool;

    fn clear_focus(&mut self);

    fn begin_focused_activation(&mut self) -> bool;

    fn complete_focused_activation(&mut self) -> Result<bool, ListenerInvokeError>;

    fn begin_activation_at(&mut self, position: Point) -> bool;

    fn complete_activation_at(&mut self, position: Point) -> Result<bool, ListenerInvokeError>;

    fn cancel_activation(&mut self);

    fn scroll_at(&mut self, position: Point, delta: Offset) -> bool;
}

/// platform-facing rendering operations.
///
/// this interface is backend-independent. It knows about the runtime's abstract resource
/// type and `ResourcePainter`, but not about embedded-graphics, framebuffers, SDL, e-ink, or any other backend.
pub trait RenderRuntimeApi {
    type Resources: TextMeasurer;

    fn rebuild(&mut self) -> Result<(), FrameBuildError>;

    fn layout(&mut self, viewport: Size) -> Option<Size>;

    fn take_render_invalidation(&self) -> RenderInvalidation;

    fn paint_with_damage<P>(
        &mut self,
        damage: DamageRegion,
        painter: &mut P,
    ) -> Result<Option<()>, P::Error>
    where
        P: ResourcePainter<Self::Resources>;
}

impl<
    const EB: usize,
    const ES: usize,
    const CB: usize,
    const CS: usize,
    const FN: usize,
    const FT: usize,
    const ST: usize,
    const GB: usize,
    const GS: usize,
    RESOURCES,
> RuntimeApi for Runtime<EB, ES, CB, CS, FN, FT, ST, GB, GS, RESOURCES>
{
    fn update<T, R, F>(&self, entity: Entity<T>, update: F) -> Result<R, EntityAccessError>
    where
        T: 'static,
        F: FnOnce(&mut T, &mut Context<'_, T>) -> R,
    {
        self.update(entity, update)
    }

    fn focus_previous(&mut self) -> bool {
        self.focus_previous()
    }

    fn focus_next(&mut self) -> bool {
        self.focus_next()
    }

    fn clear_focus(&mut self) {
        self.clear_focus();
    }

    fn begin_focused_activation(&mut self) -> bool {
        self.begin_focused_activation()
    }

    fn complete_focused_activation(&mut self) -> Result<bool, ListenerInvokeError> {
        self.complete_focused_activation()
    }

    fn begin_activation_at(&mut self, position: Point) -> bool {
        self.begin_activation_at(position)
    }

    fn complete_activation_at(&mut self, position: Point) -> Result<bool, ListenerInvokeError> {
        self.complete_activation_at(position)
    }

    fn cancel_activation(&mut self) {
        self.cancel_activation();
    }

    fn scroll_at(&mut self, position: Point, delta: Offset) -> bool {
        self.scroll_at(position, delta)
    }
}

impl<
    const EB: usize,
    const ES: usize,
    const CB: usize,
    const CS: usize,
    const FN: usize,
    const FT: usize,
    const ST: usize,
    const GB: usize,
    const GS: usize,
    RESOURCES,
> RenderRuntimeApi for Runtime<EB, ES, CB, CS, FN, FT, ST, GB, GS, RESOURCES>
where
    RESOURCES: TextMeasurer,
{
    type Resources = RESOURCES;

    fn rebuild(&mut self) -> Result<(), FrameBuildError> {
        self.rebuild()
    }

    fn layout(&mut self, viewport: Size) -> Option<Size> {
        self.layout(viewport)
    }

    fn take_render_invalidation(&self) -> RenderInvalidation {
        self.take_render_invalidation()
    }

    fn paint_with_damage<P>(
        &mut self,
        damage: DamageRegion,
        painter: &mut P,
    ) -> Result<Option<()>, P::Error>
    where
        P: ResourcePainter<Self::Resources>,
    {
        self.paint_with_damage(damage, painter)
    }
}

#[cfg(test)]
mod tests {
    use crate::{Entity, RenderRuntimeApi, RuntimeApi, RuntimeBuilder};

    fn increment_through_api(runtime: &impl RuntimeApi, entity: Entity<u32>) -> u32 {
        runtime
            .update(entity, |value, _| {
                *value += 1;
                *value
            })
            .unwrap()
    }

    fn focus_next_through_api(runtime: &mut impl RuntimeApi) -> bool {
        runtime.focus_next()
    }

    fn render_is_clean(runtime: &impl RenderRuntimeApi) -> bool {
        runtime.take_render_invalidation().is_none()
    }

    #[test]
    fn runtime_api_updates_entities_without_exposing_capacities() {
        let runtime = RuntimeBuilder::default()
            .entities::<256, 4>()
            .callbacks::<256, 4>()
            .frame::<16, 256>()
            .element_states::<8>()
            .build();

        let entity = runtime.create(|_| 41u32).unwrap();

        assert_eq!(increment_through_api(&runtime, entity,), 42,);

        assert_eq!(increment_through_api(&runtime, entity,), 43,);
    }

    #[test]
    fn runtime_api_exposes_interaction_without_capacities() {
        let mut runtime = RuntimeBuilder::default()
            .entities::<256, 4>()
            .callbacks::<256, 4>()
            .frame::<16, 256>()
            .element_states::<8>()
            .build();

        assert!(!focus_next_through_api(&mut runtime,),);
    }

    #[test]
    fn render_runtime_api_hides_concrete_runtime_type() {
        let runtime = RuntimeBuilder::default()
            .entities::<256, 4>()
            .callbacks::<256, 4>()
            .frame::<16, 256>()
            .element_states::<8>()
            .render_resources::<1, 1, 16, 0>()
            .build();

        assert!(render_is_clean(&runtime,),);
    }
}
