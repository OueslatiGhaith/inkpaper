use core::{any::TypeId, cell::Cell};

#[cfg(feature = "metrics")]
use crate::PerformanceMetrics;
use crate::{
    ActivateEvent, Context, DamageRegion, Entity, EntityAllocError, EntityArena, EventTarget,
    FrameArena, Invalidation, Listener, MountError, NodeId, Offset, Painter, Point, Render,
    RenderInvalidation, Size, TextMeasurer,
    callback_store::{CallbackArena, ListenerInvokeError},
    element_state::{ElementStateId, ElementStateTable, IdentityError},
    entity_store::create_entity,
    input::ActivationState,
    px,
    scroll::ScrollStateTable,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameBuildError {
    Mount(MountError),
    Identity(IdentityError),
}

impl From<MountError> for FrameBuildError {
    fn from(value: MountError) -> Self {
        Self::Mount(value)
    }
}

impl From<IdentityError> for FrameBuildError {
    fn from(value: IdentityError) -> Self {
        Self::Identity(value)
    }
}

pub struct Runtime<
    const ENTITY_BYTES: usize,
    const ENTITY_SLOTS: usize,
    const CALLBACK_BYTES: usize,
    const CALLBACK_SLOTS: usize,
    const FRAME_NODES: usize,
    const FRAME_TEXT_BYTES: usize,
    const ELEMENT_STATES: usize,
> {
    entities: EntityArena<ENTITY_BYTES, ENTITY_SLOTS>,
    callbacks: CallbackArena<CALLBACK_BYTES, CALLBACK_SLOTS>,
    frame: FrameArena<FRAME_NODES, FRAME_TEXT_BYTES>,
    element_states: ElementStateTable<ELEMENT_STATES>,
    scroll_states: ScrollStateTable<ELEMENT_STATES>,
    notified: Cell<bool>,
    visual_invalidation: Cell<RenderInvalidation>,
    frame_generation: u32,
    root: Option<NodeId>,
    activation: ActivationState,
    focused: Option<ElementStateId>,
    pending_scroll_into_view: Option<ElementStateId>,
}

impl<
    const EB: usize,
    const ES: usize,
    const CB: usize,
    const CS: usize,
    const FN: usize,
    const FT: usize,
    const ST: usize,
> Default for Runtime<EB, ES, CB, CS, FN, FT, ST>
{
    fn default() -> Self {
        Self {
            entities: EntityArena::default(),
            callbacks: CallbackArena::default(),
            frame: FrameArena::default(),
            element_states: ElementStateTable::default(),
            scroll_states: ScrollStateTable::default(),
            notified: Cell::new(false),
            visual_invalidation: Cell::new(RenderInvalidation::none()),
            frame_generation: 0,
            root: None,
            activation: ActivationState::default(),
            focused: None,
            pending_scroll_into_view: None,
        }
    }
}

impl<
    const EB: usize,
    const ES: usize,
    const LB: usize,
    const LS: usize,
    const FN: usize,
    const FT: usize,
    const ST: usize,
> Runtime<EB, ES, LB, LS, FN, FT, ST>
{
    pub fn create<T>(
        &self,
        build: impl FnOnce(&mut Context<'_, T>) -> T,
    ) -> Result<Entity<T>, EntityAllocError>
    where
        T: 'static,
    {
        create_entity(&self.entities, &self.callbacks, &self.notified, build)
    }

    fn next_frame_generation(&mut self) -> u32 {
        let next = self.frame_generation.wrapping_add(1);

        if next == 0 {
            // `last_seen_frame` is u32, so don't allow old frame numbers to alias
            // after wrap around
            self.element_states.clear();
            self.frame_generation = 1;
        } else {
            self.frame_generation = next;
        }

        self.frame_generation
    }

    pub fn rebuild<T>(&mut self, root: Entity<T>) -> Result<(), FrameBuildError>
    where
        T: Render,
    {
        self.root = None;
        self.frame.clear();

        // invalidate every CallbackId from the previous frame
        self.callbacks.reset();
        // consume the previous dirty request.
        // if render/event logic calls notify during this build, it becomes dirty again
        self.notified.set(false);
        self.visual_invalidation.set(RenderInvalidation::none());
        let generation = self.next_frame_generation();

        let result = self.build_frame(root, generation);

        match result {
            Ok(root_node) => {
                self.element_states.sweep(generation);
                self.root = Some(root_node);
                self.reconcile_interaction_state();
                self.refresh_interaction_styles();
                self.frame.resolve_scroll_offsets(&self.scroll_states);

                Ok(())
            }
            Err(error) => {
                // identity resolution may have partially touched persistent state
                self.element_states.abort_frame(generation);
                // never expose a partial frame
                self.frame.clear();
                // never keep callbacks registered by a failed render
                self.callbacks.reset();
                self.activation.cancel();
                self.visual_invalidation.set(RenderInvalidation::none());
                self.root = None;
                self.focused = None;
                self.pending_scroll_into_view = None;

                Err(error)
            }
        }
    }

    fn build_frame<T>(
        &mut self,
        root: Entity<T>,
        generation: u32,
    ) -> Result<NodeId, FrameBuildError>
    where
        T: Render,
    {
        let root_node =
            self.frame
                .mount_and_expand(root, &self.entities, &self.callbacks, &self.notified)?;

        self.frame
            .resolve_identities(&mut self.element_states, generation)?;

        Ok(root_node)
    }

    pub(crate) fn root_node(&self) -> Option<NodeId> {
        self.root
    }

    pub(crate) fn frame(&self) -> &FrameArena<FN, FT> {
        &self.frame
    }

    pub fn frame_node_count(&self) -> usize {
        self.frame.node_count()
    }

    pub fn frame_text_bytes_used(&self) -> usize {
        self.frame.text_bytes_used()
    }

    pub(crate) fn is_dirty(&self) -> bool {
        self.invalidation() != Invalidation::None
    }

    pub(crate) fn take_dirty(&self) -> bool {
        self.take_invalidation() != Invalidation::None
    }

    pub(crate) fn invoke<E>(
        &self,
        listener: Listener<E>,
        event: &E,
    ) -> Result<(), ListenerInvokeError>
    where
        E: 'static,
    {
        self.callbacks
            .invoke_listener(listener, event, &self.entities, &self.notified)
    }

    pub fn layout(&mut self, viewport: Size, text_measurer: &dyn TextMeasurer) -> Option<Size> {
        let root = self.root?;
        let size = self.frame.layout(root, viewport, text_measurer);
        self.frame.clamp_scroll_offset(&mut self.scroll_states);

        if let Some(element) = self.pending_scroll_into_view.take() {
            self.frame
                .scroll_element_into_view(root, element, &mut self.scroll_states);
        }

        Some(size)
    }

    pub fn paint<P>(&self, painter: &mut P) -> Result<Option<()>, P::Error>
    where
        P: Painter,
    {
        let Some(root) = self.root else {
            return Ok(None);
        };

        self.frame
            .paint_with_runtime(root, &self.entities, &self.callbacks, painter)?;

        Ok(Some(()))
    }

    pub fn paint_with_damage<P>(
        &self,
        damage: DamageRegion,
        painter: &mut P,
    ) -> Result<Option<()>, P::Error>
    where
        P: Painter,
    {
        let Some(root) = self.root else {
            return Ok(None);
        };

        self.frame.paint_with_runtime_and_damage(
            root,
            &self.entities,
            &self.callbacks,
            damage,
            painter,
        )?;

        Ok(Some(()))
    }

    fn reconcile_interaction_state(&mut self) {
        if let Some(focused) = self.focused {
            let still_focusable = self
                .root
                .and_then(|root| self.frame.focus_target_for_element(root, focused))
                .is_some();

            if !still_focusable {
                self.focused = None;
                self.pending_scroll_into_view = None;
            }
        }

        if let Some(pending) = self.pending_scroll_into_view {
            let still_focusable = self
                .root
                .and_then(|root| self.frame.focus_target_for_element(root, pending))
                .is_some();

            if !still_focusable {
                self.pending_scroll_into_view = None;
            }
        }

        if let Some(pressed) = self.activation.pressed()
            && self.activation_target_for_element(pressed).is_none()
        {
            self.activation.cancel();
        }
    }

    pub fn focus_next(&mut self) -> bool {
        let Some(root) = self.root else {
            let invalidation = self.set_focus(None);
            self.invalidate_render(invalidation);
            return false;
        };

        let Some(target) = self.frame.next_focus_target(root, self.focused) else {
            let invalidation = self.set_focus(None);
            self.invalidate_render(invalidation);
            return false;
        };

        let invalidation = self.set_focus(Some(target.element));
        self.invalidate_render(invalidation);

        true
    }

    pub fn focus_previous(&mut self) -> bool {
        let Some(root) = self.root else {
            let invalidation = self.set_focus(None);
            self.invalidate_render(invalidation);
            return false;
        };

        let Some(target) = self.frame.previous_focus_target(root, self.focused) else {
            let invalidation = self.set_focus(None);
            self.invalidate_render(invalidation);
            return false;
        };

        let invalidation = self.set_focus(Some(target.element));
        self.invalidate_render(invalidation);

        true
    }

    pub fn clear_focus(&mut self) {
        let invalidation = self.set_focus(None);
        self.invalidate_render(invalidation);
    }

    pub fn activate_focused(&mut self) -> Result<bool, ListenerInvokeError> {
        self.dispatch_to_focused(&ActivateEvent)
    }

    fn invalidate_render(&self, invalidation: RenderInvalidation) {
        let current = self.visual_invalidation.get();
        self.visual_invalidation.set(current.merge(invalidation));
    }

    pub fn render_invalidation(&self) -> RenderInvalidation {
        let visual = self.visual_invalidation.get();
        if self.notified.get() {
            visual.merge(RenderInvalidation::full(Invalidation::Rebuild))
        } else {
            visual
        }
    }

    pub fn invalidation(&self) -> Invalidation {
        self.render_invalidation().kind()
    }

    pub fn damage(&self) -> DamageRegion {
        self.render_invalidation().damage()
    }

    #[cfg(feature = "metrics")]
    fn record_render_invalidation_metrics(&self, invalidation: RenderInvalidation) {
        if invalidation.is_none() {
            return;
        }

        let damage = invalidation.damage();
        self.frame.metrics.increment(|metrics| {
            metrics.render_invalidations_consumed += 1;
            if damage.is_full() {
                metrics.full_damage_invalidations += 1;
            } else {
                metrics.partial_damage_invalidations += 1;

                let rectangle_count = damage.len() as u64;
                metrics.damage_rectangles += rectangle_count;
            }
        });
    }

    pub fn take_render_invalidation(&self) -> RenderInvalidation {
        let visual = self.visual_invalidation.replace(RenderInvalidation::none());

        let application = if self.notified.replace(false) {
            RenderInvalidation::full(Invalidation::Rebuild)
        } else {
            RenderInvalidation::none()
        };

        let invalidation = visual.merge(application);

        #[cfg(feature = "metrics")]
        self.record_render_invalidation_metrics(invalidation);

        invalidation
    }

    pub fn take_invalidation(&self) -> Invalidation {
        self.take_render_invalidation().kind()
    }

    fn refresh_interaction_styles(&mut self) {
        self.frame
            .resolve_interaction_styles(self.focused, self.activation.pressed());

        if let Some(root) = self.root {
            self.frame.resolve_text_styles(root);
        }
    }

    fn focus_transition_invalidation(
        &self,
        previous: Option<ElementStateId>,
        next: Option<ElementStateId>,
    ) -> Invalidation {
        if previous == next {
            return Invalidation::None;
        }

        let mut invalidation = Invalidation::None;
        if let Some(previous) = previous {
            invalidation = invalidation.merge(self.frame.focused_style_invalidation(previous));
        }
        if let Some(next) = next {
            invalidation = invalidation.merge(self.frame.focused_style_invalidation(next));
        }

        invalidation
    }

    fn pressed_transition_invalidation(
        &self,
        previous: Option<ElementStateId>,
        next: Option<ElementStateId>,
    ) -> Invalidation {
        if previous == next {
            return Invalidation::None;
        }

        let mut invalidation = Invalidation::None;
        if let Some(previous) = previous {
            invalidation = invalidation.merge(self.frame.pressed_style_invalidation(previous));
        }
        if let Some(next) = next {
            invalidation = invalidation.merge(self.frame.pressed_style_invalidation(next));
        }

        invalidation
    }

    fn interaction_damage(
        &self,
        previous: Option<ElementStateId>,
        next: Option<ElementStateId>,
    ) -> Option<DamageRegion> {
        let root = self.root?;
        self.frame.visual_damage_for_element(root, previous, next)
    }

    fn finish_interaction_invalidation(
        &self,
        kind: Invalidation,
        before_damage: Option<DamageRegion>,
        previous: Option<ElementStateId>,
        next: Option<ElementStateId>,
    ) -> RenderInvalidation {
        match kind {
            Invalidation::None => RenderInvalidation::none(),
            Invalidation::Paint => {
                let Some(before_damage) = before_damage else {
                    return RenderInvalidation::full(Invalidation::Paint);
                };
                let Some(after_damage) = self.interaction_damage(previous, next) else {
                    return RenderInvalidation::full(Invalidation::Paint);
                };

                let damage = before_damage.merge(after_damage);
                if damage.is_none() {
                    RenderInvalidation::full(Invalidation::Paint)
                } else {
                    RenderInvalidation::damaged(Invalidation::Paint, damage)
                }
            }
            Invalidation::Layout => RenderInvalidation::full(Invalidation::Layout),
            Invalidation::Rebuild => RenderInvalidation::full(Invalidation::Rebuild),
        }
    }

    pub fn scroll_at(&mut self, position: Point, delta: Offset) -> bool {
        let Some(root) = self.root else {
            return false;
        };
        let Some(target) = self.frame.hit_test_scroll(root, position) else {
            return false;
        };

        let previous = self.scroll_states.offset(target.element);

        let next_x = if target.axes.horizontal() {
            (previous.x + delta.x).clamp(px(0), target.max_offset.x)
        } else {
            previous.x
        };
        let next_y = if target.axes.vertical() {
            (previous.y + delta.y).clamp(px(0), target.max_offset.y)
        } else {
            previous.y
        };

        let next = Offset::new(next_x, next_y);
        if next == previous {
            return false;
        }

        self.scroll_states.set_offset(target.element, next);
        self.frame.set_scroll_offset(target.node, next);

        if !target.damage.is_none() {
            self.invalidate_render(RenderInvalidation::damaged(
                Invalidation::Paint,
                target.damage,
            ));
        }

        true
    }

    fn scroll_element_into_view_now(&mut self, element: ElementStateId) -> DamageRegion {
        let Some(root) = self.root else {
            return DamageRegion::none();
        };
        self.frame
            .scroll_element_into_view(root, element, &mut self.scroll_states)
    }

    fn set_focus(&mut self, next: Option<ElementStateId>) -> RenderInvalidation {
        let previous = self.focused;
        if previous == next {
            if let Some(element) = next {
                let scroll_damage = self.scroll_element_into_view_now(element);
                if !scroll_damage.is_none() {
                    return RenderInvalidation::damaged(Invalidation::Paint, scroll_damage);
                }
            }

            return RenderInvalidation::none();
        }

        let kind = self.focus_transition_invalidation(previous, next);
        let before_damage = if kind == Invalidation::Paint {
            self.interaction_damage(previous, next)
        } else {
            None
        };

        self.focused = next;
        self.refresh_interaction_styles();
        self.pending_scroll_into_view = None;

        let mut invalidation =
            self.finish_interaction_invalidation(kind, before_damage, previous, next);
        let Some(element) = next else {
            return invalidation;
        };

        if matches!(kind, Invalidation::Layout | Invalidation::Rebuild) {
            // focus styling changed geometry, current frame bounds are stale,
            // so wait until layout has recomputed them
            self.pending_scroll_into_view = Some(element);
        } else {
            let scroll_damage = self.scroll_element_into_view_now(element);
            if !scroll_damage.is_none() {
                invalidation = invalidation.merge(RenderInvalidation::damaged(
                    Invalidation::Paint,
                    scroll_damage,
                ));
            }
        }

        invalidation
    }

    fn dispatch_to_node<E>(&self, node: NodeId, event: &E) -> Result<bool, ListenerInvokeError>
    where
        E: 'static,
    {
        let mut handled = false;

        for callback in self.frame.event_callbacks(node, TypeId::of::<E>()) {
            let listener = Listener::from_id(callback);
            self.invoke(listener, event)?;
            handled = true;
        }

        Ok(handled)
    }

    pub fn dispatch_to_focused<E>(&self, event: &E) -> Result<bool, ListenerInvokeError>
    where
        E: 'static,
    {
        let Some(target) = self.focused_target() else {
            return Ok(false);
        };

        self.dispatch_to(target, event)
    }

    pub fn dispatch_at<E>(&self, position: Point, event: &E) -> Result<bool, ListenerInvokeError>
    where
        E: 'static,
    {
        let Some(target) = self.target_at::<E>(position) else {
            return Ok(false);
        };

        self.dispatch_to(target, event)
    }

    fn activation_target_for_element(&self, element: ElementStateId) -> Option<EventTarget> {
        let target = EventTarget::new(element);
        let node = self.node_for_target(target)?;

        self.frame
            .event_callbacks(node, TypeId::of::<ActivateEvent>())
            .next()?;

        Some(target)
    }

    fn activation_target_at(&self, position: Point) -> Option<EventTarget> {
        self.target_at::<ActivateEvent>(position)
    }

    fn set_pressed(&mut self, next: Option<ElementStateId>) -> RenderInvalidation {
        let previous = self.activation.pressed();
        if previous == next {
            return RenderInvalidation::none();
        }

        let kind = self.pressed_transition_invalidation(previous, next);
        let before_damage = if kind == Invalidation::Paint {
            self.interaction_damage(previous, next)
        } else {
            None
        };

        self.activation.set_pressed(next);
        self.refresh_interaction_styles();

        self.finish_interaction_invalidation(kind, before_damage, previous, next)
    }

    pub fn begin_activation_at(&mut self, position: Point) -> bool {
        let target = self.activation_target_at(position);
        let next = target.map(EventTarget::element);
        let invalidation = self.set_pressed(next);

        self.invalidate_render(invalidation);

        target.is_some()
    }

    pub fn complete_activation_at(&mut self, position: Point) -> Result<bool, ListenerInvokeError> {
        let Some(pressed) = self.activation.pressed() else {
            return Ok(false);
        };

        let target = self.activation_target_at(position);
        let activated = target.map(EventTarget::element) == Some(pressed);

        let pressed_invalidation = self.set_pressed(None);
        let focus_invalidation = if activated {
            self.set_focus(Some(pressed))
        } else {
            RenderInvalidation::none()
        };

        self.invalidate_render(pressed_invalidation.merge(focus_invalidation));

        let Some(target) = target.filter(|target| target.element() == pressed) else {
            return Ok(false);
        };

        self.dispatch_to(target, &ActivateEvent)
    }

    pub fn cancel_activation(&mut self) {
        let invalidation = self.set_pressed(None);
        self.invalidate_render(invalidation);
    }

    pub fn begin_focused_activation(&mut self) -> bool {
        let target = self
            .focused
            .and_then(|element| self.activation_target_for_element(element));
        let next = target.map(EventTarget::element);
        let invalidation = self.set_pressed(next);

        self.invalidate_render(invalidation);

        next.is_some()
    }

    pub fn complete_focused_activation(&mut self) -> Result<bool, ListenerInvokeError> {
        let Some(pressed) = self.activation.pressed() else {
            return Ok(false);
        };
        let target = if self.focused == Some(pressed) {
            self.activation_target_for_element(pressed)
        } else {
            None
        };

        let invalidation = self.set_pressed(None);

        self.invalidate_render(invalidation);

        let Some(target) = target else {
            return Ok(false);
        };

        self.dispatch_to(target, &ActivateEvent)
    }

    fn node_for_target(&self, target: EventTarget) -> Option<NodeId> {
        let root = self.root?;
        self.frame.node_for_element(root, target.element())
    }

    pub fn focused_target(&self) -> Option<EventTarget> {
        let focused = self.focused?;
        let target = EventTarget::new(focused);
        self.node_for_target(target)?;

        Some(target)
    }

    pub fn target_at<E>(&self, position: Point) -> Option<EventTarget>
    where
        E: 'static,
    {
        let root = self.root?;
        let node = self
            .frame
            .hit_test_event(root, position, TypeId::of::<E>())?;
        let element = self.frame.node(node).element_state_id?;

        Some(EventTarget::new(element))
    }

    pub fn dispatch_to<E>(
        &self,
        target: EventTarget,
        event: &E,
    ) -> Result<bool, ListenerInvokeError>
    where
        E: 'static,
    {
        let Some(node) = self.node_for_target(target) else {
            return Ok(false);
        };

        self.dispatch_to_node(node, event)
    }

    #[cfg(feature = "metrics")]
    pub fn reset_performance_metrics(&self) {
        self.frame.reset_performance_metrics();
    }

    #[cfg(feature = "metrics")]
    pub fn performance_metrics(&self) -> PerformanceMetrics {
        self.frame.performance_metrics()
    }
}
