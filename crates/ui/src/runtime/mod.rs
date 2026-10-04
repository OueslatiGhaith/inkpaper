use core::{any::TypeId, cell::Cell};

#[cfg(feature = "alloc")]
use alloc::boxed::Box;

use crate::{
    ActivateEvent, Context, DamageRegion, ElementId, Entity, EntityAccessError, EntityAllocError,
    EventTarget, FontFace, FontFamilyId, FontId, FontRegistryError, FrameArena, ImageRegistryError,
    ImageResource, ImageSource, Invalidation, Listener, MountError, NodeId, Offset, PaintReport,
    Point, Render, RenderInvalidation, ResourcePainter, RuntimeStorage, Size, TextMeasurer,
    callback::ListenerInvokeError,
    element::state::{ElementStateId, ElementStateTable, IdentityError},
    entity::create_entity,
    global::{Global, GlobalAccessError, GlobalMut, GlobalRef, GlobalSetError},
    interaction::{input::ActivationState, scroll::ScrollStateTable},
    px,
    resources::RuntimeResources,
    runtime::root::RuntimeRoot,
    storage::{CallbackStorage, GlobalStorage},
};
#[cfg(feature = "metrics")]
use crate::{GlyphCacheMetrics, PerformanceMetrics};

mod api;
mod cx;
mod root;

pub use api::{RenderRuntimeApi, ResourceRuntimeApi, RuntimeApi};
pub(crate) use cx::RuntimeCx;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameBuildError {
    RootNotSet,
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

/// the UI runtime. `S` chooses the storage behind each runtime table (see
/// [`RuntimeStorage`]), and `RESOURCES` holds render resources such as fonts and images.
pub struct Runtime<S, RESOURCES = ()>
where
    S: RuntimeStorage,
{
    entities: S::Entities,
    globals: S::Globals,
    callbacks: S::Callbacks,
    frame: FrameArena<S::Frame>,
    element_states: ElementStateTable<S::ElementStates>,
    scroll_states: ScrollStateTable<S::ElementStates>,
    resources: RESOURCES,

    /// every entity needs to render again. Entities changed one at a time are marked
    /// in `entities`
    all_dirty: Cell<bool>,
    visual_invalidation: Cell<RenderInvalidation>,
    frame_generation: u32,
    root_entity: Option<RuntimeRoot>,
    root: Option<NodeId>,
    activation: ActivationState,
    focused: Option<ElementStateId>,
    pending_scroll_into_view: Option<ElementStateId>,
}

impl<S, RESOURCES> Default for Runtime<S, RESOURCES>
where
    S: RuntimeStorage,
    RESOURCES: Default,
{
    fn default() -> Self {
        Self {
            entities: S::Entities::default(),
            globals: S::Globals::default(),
            callbacks: S::Callbacks::default(),
            frame: FrameArena::default(),
            element_states: ElementStateTable::default(),
            scroll_states: ScrollStateTable::default(),
            resources: RESOURCES::default(),
            all_dirty: Cell::new(false),
            visual_invalidation: Cell::new(RenderInvalidation::none()),
            frame_generation: 0,
            root_entity: None,
            root: None,
            activation: ActivationState::default(),
            focused: None,
            pending_scroll_into_view: None,
        }
    }
}

impl<S, RESOURCES> Runtime<S, RESOURCES>
where
    S: RuntimeStorage,
{
    pub fn create<T>(
        &self,
        build: impl FnOnce(&mut Context<'_, T>) -> T,
    ) -> Result<Entity<T>, EntityAllocError>
    where
        T: 'static,
    {
        create_entity(self.runtime_cx(), build)
    }

    pub fn create_root<T>(
        &mut self,
        build: impl FnOnce(&mut Context<'_, T>) -> T,
    ) -> Result<Entity<T>, EntityAllocError>
    where
        T: Render,
    {
        let entity = self.create(build)?;
        self.set_root(entity);

        Ok(entity)
    }

    pub fn set_root<T>(&mut self, root: Entity<T>)
    where
        T: Render,
    {
        self.root_entity = Some(RuntimeRoot::from_entity(root));

        // changing the registered root requires a full rebuild, but keep the currently
        // mounted frame alive until that rebuild occurs.
        self.all_dirty.set(true);
    }

    pub fn update<T, R>(
        &self,
        entity: Entity<T>,
        update: impl FnOnce(&mut T, &mut Context<'_, T>) -> R,
    ) -> Result<R, EntityAccessError>
    where
        T: 'static,
    {
        let cx = Context::new_in(entity, self.runtime_cx());

        entity.update(&cx, update)
    }

    fn runtime_cx(&self) -> RuntimeCx<'_> {
        RuntimeCx::new(
            &self.entities,
            &self.globals,
            &self.callbacks,
            &self.all_dirty,
        )
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

    pub fn rebuild(&mut self) -> Result<(), FrameBuildError> {
        let root_entity = self.root_entity.ok_or(FrameBuildError::RootNotSet)?;

        self.root = None;
        self.frame.clear();

        // invalidate every CallbackId from the previous frame
        self.callbacks.reset();
        // consume the previous dirty request.
        // if render/event logic calls notify during this build, it becomes dirty again
        self.runtime_cx().clear_dirty();
        self.visual_invalidation.set(RenderInvalidation::none());
        let generation = self.next_frame_generation();

        let result = self.build_frame(root_entity, generation);

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

    fn build_frame(
        &mut self,
        root: RuntimeRoot,
        generation: u32,
    ) -> Result<NodeId, FrameBuildError> {
        let runtime = RuntimeCx::new(
            &self.entities,
            &self.globals,
            &self.callbacks,
            &self.all_dirty,
        );
        let root_node = self.frame.mount_and_expand(root, runtime)?;

        self.frame
            .resolve_identities(root_node, &mut self.element_states, generation)?;
        self.scroll_states
            .prepare(self.element_states.slot_count())?;

        Ok(root_node)
    }

    pub(crate) fn root_node(&self) -> Option<NodeId> {
        self.root
    }

    pub(crate) fn frame(&self) -> &FrameArena<S::Frame> {
        &self.frame
    }

    pub fn frame_node_count(&self) -> usize {
        self.frame.node_count()
    }

    pub fn frame_text_bytes_used(&self) -> usize {
        self.frame.text_bytes_used()
    }

    /// releases spare frame capacity while preserving the current frame.
    ///
    /// call after rebuilding a smaller screen, rather than after every frame. The allocator
    /// may retain some capacity. Does nothing for fixed storage
    pub fn shrink_frame_storage(&mut self) {
        self.frame.shrink_to_fit();
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
            .invoke_listener(listener, event, self.runtime_cx())
    }

    pub fn layout_with_measurer(
        &mut self,
        viewport: Size,
        text_measurer: &dyn TextMeasurer,
    ) -> Option<Size> {
        let root = self.root?;
        let size = self.frame.layout(root, viewport, text_measurer);
        self.frame
            .apply_initial_scroll_offsets(&mut self.scroll_states);
        self.frame.clamp_scroll_offset(&mut self.scroll_states);

        if let Some(element) = self.pending_scroll_into_view.take() {
            self.frame
                .scroll_element_into_view(root, element, &mut self.scroll_states);
        }

        Some(size)
    }

    pub fn paint<P>(&mut self, painter: &mut P) -> Result<Option<PaintReport>, P::Error>
    where
        P: ResourcePainter<RESOURCES>,
    {
        let Some(root) = self.root else {
            return Ok(None);
        };

        let report = self.frame.paint_with_runtime(
            root,
            &self.entities,
            &self.callbacks,
            &mut self.resources,
            painter,
        )?;

        Ok(Some(report))
    }

    pub fn paint_with_damage<P>(
        &mut self,
        damage: DamageRegion,
        painter: &mut P,
    ) -> Result<Option<PaintReport>, P::Error>
    where
        P: ResourcePainter<RESOURCES>,
    {
        let Some(root) = self.root else {
            return Ok(None);
        };

        let report = self.frame.paint_with_runtime_and_damage(
            root,
            &self.entities,
            &self.callbacks,
            &mut self.resources,
            damage,
            painter,
        )?;

        Ok(Some(report))
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
        let Some(target) = self.focused_target() else {
            return Ok(false);
        };

        self.activate(target)
    }

    /// Sends `target` an [`ActivateEvent`] carrying its element id.
    fn activate(&self, target: EventTarget) -> Result<bool, ListenerInvokeError> {
        self.dispatch_to_with(target, ActivateEvent::new)
    }

    fn invalidate_render(&self, invalidation: RenderInvalidation) {
        let current = self.visual_invalidation.get();
        self.visual_invalidation.set(current.merge(invalidation));
    }

    pub fn render_invalidation(&self) -> RenderInvalidation {
        let visual = self.visual_invalidation.get();
        if self.runtime_cx().is_dirty() {
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

        let runtime = self.runtime_cx();
        let application = if runtime.is_dirty() {
            runtime.clear_dirty();
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
            .set_interaction_state(self.focused, self.activation.pressed());

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

    /// Like [`Self::dispatch_at`], but builds the event from the id of the element
    /// that listens for it, so one listener can serve many elements.
    pub fn dispatch_at_with<E>(
        &self,
        position: Point,
        event: impl FnOnce(ElementId) -> E,
    ) -> Result<bool, ListenerInvokeError>
    where
        E: 'static,
    {
        let Some(target) = self.target_at::<E>(position) else {
            return Ok(false);
        };

        self.dispatch_to_with(target, event)
    }

    fn dispatch_to_with<E>(
        &self,
        target: EventTarget,
        event: impl FnOnce(ElementId) -> E,
    ) -> Result<bool, ListenerInvokeError>
    where
        E: 'static,
    {
        let Some(entry) = self.element_states.entry(target.element()) else {
            return Ok(false);
        };

        self.dispatch_to(target, &event(entry.key.local))
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

        self.activate(target)
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

        self.activate(target)
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

    pub fn set_global<G>(&mut self, value: G) -> Result<(), GlobalSetError>
    where
        G: Global,
    {
        self.globals.set(value)?;
        self.all_dirty.set(true);

        Ok(())
    }

    pub fn has_global<G>(&self) -> bool
    where
        G: Global,
    {
        self.globals.contains::<G>()
    }

    pub fn try_global<G>(&self) -> Result<GlobalRef<'_, G>, GlobalAccessError>
    where
        G: Global,
    {
        GlobalRef::acquire(&self.globals)
    }

    pub fn global<G>(&self) -> GlobalRef<'_, G>
    where
        G: Global,
    {
        self.try_global::<G>()
            .unwrap_or_else(|_| panic!("requested global is not available"))
    }

    pub fn try_global_mut<G>(&self) -> Result<GlobalMut<'_, G>, GlobalAccessError>
    where
        G: Global,
    {
        GlobalMut::acquire(&self.globals, &self.all_dirty)
    }

    pub fn global_mut<G>(&self) -> GlobalMut<'_, G>
    where
        G: Global,
    {
        self.try_global_mut::<G>()
            .unwrap_or_else(|_| panic!("requested global is not available for mutation"))
    }

    pub fn global_count(&self) -> usize {
        self.globals.len()
    }

    pub fn global_bytes_used(&self) -> usize {
        self.globals.used_bytes()
    }

    pub fn global_capacity(&self) -> usize {
        self.globals.capacity()
    }

    pub fn global_byte_capacity(&self) -> usize {
        self.globals.byte_capacity()
    }
}

impl<S, RESOURCES> Runtime<S, RESOURCES>
where
    S: RuntimeStorage,
    RESOURCES: TextMeasurer,
{
    pub fn layout(&mut self, viewport: Size) -> Option<Size> {
        let root = self.root?;
        let size = self.frame.layout(root, viewport, &self.resources);

        self.frame
            .apply_initial_scroll_offsets(&mut self.scroll_states);
        self.frame.clamp_scroll_offset(&mut self.scroll_states);

        if let Some(element) = self.pending_scroll_into_view.take() {
            self.frame
                .scroll_element_into_view(root, element, &mut self.scroll_states);
        }

        Some(size)
    }
}

impl<
    'resource,
    S,
    const FONTS: usize,
    const GLYPH_SLOTS: usize,
    const GLYPH_BYTES: usize,
    const IMAGES: usize,
> Runtime<S, RuntimeResources<'resource, FONTS, GLYPH_SLOTS, GLYPH_BYTES, IMAGES>>
where
    S: RuntimeStorage,
{
    pub fn register_font(
        &mut self,
        font: &'resource dyn FontFace,
    ) -> Result<FontId, FontRegistryError> {
        self.resources.register_font(font)
    }

    pub fn register_font_family(&mut self) -> Result<FontFamilyId, FontRegistryError> {
        self.resources.register_font_family()
    }

    pub fn register_font_face(
        &mut self,
        family: FontFamilyId,
        font: &'resource dyn FontFace,
    ) -> Result<FontId, FontRegistryError> {
        self.resources.register_font_face(family, font)
    }

    pub fn register_image(
        &mut self,
        image: &'resource dyn ImageResource,
    ) -> Result<ImageSource, ImageRegistryError> {
        self.resources.register_image(image)
    }

    #[cfg(feature = "alloc")]
    pub fn register_owned_image(
        &mut self,
        image: Box<dyn ImageResource>,
    ) -> Result<ImageSource, ImageRegistryError> {
        let source = self.resources.register_owned_image(image)?;

        self.all_dirty.set(true);

        Ok(source)
    }

    #[cfg(feature = "alloc")]
    pub fn clear_owned_images(&mut self) {
        self.resources.clear_owned_images();
        self.all_dirty.set(true);
    }

    pub fn clear_glyph_cache(&mut self) {
        self.resources.clear_glyph_cache();
    }

    pub const fn glyph_cache_capacity_bytes(&self) -> usize {
        self.resources.glyph_cache_capacity_bytes()
    }

    pub const fn glyph_cache_used_bytes(&self) -> usize {
        self.resources.glyph_cache_used_bytes()
    }

    #[cfg(feature = "metrics")]
    pub fn reset_glyph_cache_metrics(&mut self) {
        self.resources.reset_glyph_cache_metrics();
    }

    #[cfg(feature = "metrics")]
    pub const fn glyph_cache_metrics(&self) -> GlyphCacheMetrics {
        self.resources.glyph_cache_metrics()
    }
}

#[cfg(test)]
mod tests {
    use crate::{entity::EntityStore, *};

    type TestRuntime = Runtime<TestStorage<1024, 8, 1024, 8, 16, 64, 8, 64, 2>>;

    struct Counter {
        value: i32,
    }

    struct Theme;

    impl Global for Theme {}

    impl Render for Counter {
        fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
            if self.value < 0 {
                // a render that notifies keeps its entity dirty for the next frame
                cx.notify();
            }

            div()
        }
    }

    fn notify(runtime: &TestRuntime, entity: Entity<Counter>) {
        runtime
            .update(entity, |counter, cx| {
                counter.value += 1;
                cx.notify();
            })
            .unwrap();
    }

    #[test]
    fn notify_marks_only_the_notifying_entity() {
        let runtime = TestRuntime::default();
        let first = runtime.create(|_| Counter { value: 0 }).unwrap();
        let second = runtime.create(|_| Counter { value: 0 }).unwrap();

        assert_eq!(runtime.invalidation(), Invalidation::None);

        notify(&runtime, first);

        assert!(runtime.entities.is_dirty(first.entity_id()));
        assert!(!runtime.entities.is_dirty(second.entity_id()));
        assert!(!runtime.all_dirty.get());
        assert_eq!(runtime.invalidation(), Invalidation::Rebuild);
    }

    #[test]
    fn taking_the_invalidation_clears_dirty_entities() {
        let runtime = TestRuntime::default();
        let counter = runtime.create(|_| Counter { value: 0 }).unwrap();

        notify(&runtime, counter);

        assert_eq!(runtime.take_invalidation(), Invalidation::Rebuild);
        assert!(!runtime.entities.is_dirty(counter.entity_id()));
        assert_eq!(runtime.invalidation(), Invalidation::None);
    }

    #[test]
    fn global_changes_mark_every_entity() {
        let mut runtime = TestRuntime::default();

        runtime.set_global(Theme).unwrap();

        assert!(runtime.all_dirty.get());
        assert_eq!(runtime.take_invalidation(), Invalidation::Rebuild);
        assert!(!runtime.all_dirty.get());

        drop(runtime.global_mut::<Theme>());

        assert!(runtime.all_dirty.get());
    }

    #[test]
    fn rebuild_clears_dirty_entities_except_those_notified_while_rendering() {
        let mut runtime = TestRuntime::default();
        let root = runtime.create_root(|_| Counter { value: 0 }).unwrap();

        notify(&runtime, root);
        runtime.rebuild().unwrap();

        assert!(!runtime.entities.is_dirty(root.entity_id()));
        assert!(!runtime.all_dirty.get());
        assert_eq!(runtime.invalidation(), Invalidation::None);

        runtime
            .update(root, |counter, _| counter.value = -1)
            .unwrap();
        runtime.rebuild().unwrap();

        assert!(runtime.entities.is_dirty(root.entity_id()));
        assert_eq!(runtime.invalidation(), Invalidation::Rebuild);
    }
}
