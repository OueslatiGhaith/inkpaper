use crate::{
    Context, Entity, EntityAccessError, Offset, Point, Runtime, callback::ListenerInvokeError,
};

pub trait RuntimeApi {
    fn update<T, R, F>(&self, entity: Entity<T>, update: F) -> Result<R, EntityAccessError>
    where
        T: 'static,
        F: FnOnce(&mut T, &mut Context<'_, T>) -> R;

    fn focus_previous(&mut self) -> bool;

    fn focus_next(&mut self) -> bool;

    fn begin_focused_activation(&mut self) -> bool;

    fn complete_focused_activation(&mut self) -> Result<bool, ListenerInvokeError>;

    fn begin_activation_at(&mut self, position: Point) -> bool;

    fn complete_activation_at(&mut self, position: Point) -> Result<bool, ListenerInvokeError>;

    fn scroll_at(&mut self, position: Point, delta: Offset) -> bool;
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

    fn scroll_at(&mut self, position: Point, delta: Offset) -> bool {
        self.scroll_at(position, delta)
    }
}

#[cfg(test)]
mod tests {
    use crate::{Entity, RuntimeApi, RuntimeBuilder};

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

        assert!(!focus_next_through_api(&mut runtime,));
    }
}
