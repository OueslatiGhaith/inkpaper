use core::{any::TypeId, cell::Cell};

use crate::{
    ClickEvent, Context, Entity, EntityAllocError, EntityArena, FrameArena, Invalidation, Listener,
    MountError, NodeId, Offset, Painter, Point, Render, Size, TextMeasurer,
    callback_store::{CallbackArena, ListenerInvokeError},
    element_state::{ElementStateId, ElementStateTable, IdentityError},
    entity_store::create_entity,
    input::PointerState,
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
    visual_invalidation: Cell<Invalidation>,
    frame_generation: u32,
    root: Option<NodeId>,
    pointer: PointerState,
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
            visual_invalidation: Cell::new(Invalidation::None),
            frame_generation: 0,
            root: None,
            pointer: PointerState::default(),
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
        self.visual_invalidation.set(Invalidation::None);
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
                self.pointer.cancel();
                self.visual_invalidation.set(Invalidation::None);
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

    pub fn pointer_down(&mut self, position: Point) -> bool {
        let previous = self.pointer.pressed();
        let Some(root) = self.root else {
            if previous.is_some() {
                let invalidation = self.pressed_transition_invalidation(previous, None);
                self.pointer.cancel();
                self.refresh_interaction_styles();
                self.invalidate(invalidation);
            }

            return false;
        };

        let target = self.frame.hit_test_click(root, position);
        let next = target.map(|target| target.element);
        if previous != next {
            let invalidation = self.pressed_transition_invalidation(previous, next);
            self.pointer.press(next);
            self.refresh_interaction_styles();
            self.invalidate(invalidation);
        }

        target.is_some()
    }

    pub fn pointer_up(&mut self, position: Point) -> Result<bool, ListenerInvokeError> {
        let Some(pressed) = self.pointer.take_pressed() else {
            return Ok(false);
        };

        let mut next_focus = self.focused;
        let mut listener = None;
        let mut activated = false;

        if let Some(root) = self.root
            && let Some(target) = self.frame.hit_test_click(root, position)
            && target.element == pressed
        {
            next_focus = Some(target.element);
            listener = Some(target.listener);
            activated = true;
        }

        let pressed_invalidation = self.pressed_transition_invalidation(Some(pressed), None);
        let focus_invalidation = self.set_focus(next_focus);

        self.invalidate(pressed_invalidation.merge(focus_invalidation));

        if let Some(listener) = listener {
            let listener = Listener::from_id(listener);
            self.callbacks.invoke_listener(
                listener,
                &ClickEvent,
                &self.entities,
                &self.notified,
            )?;
        }

        Ok(activated)
    }

    pub fn pointer_cancel(&mut self) {
        let Some(previous) = self.pointer.pressed() else {
            return;
        };

        let invalidation = self.pressed_transition_invalidation(Some(previous), None);
        self.pointer.cancel();
        self.refresh_interaction_styles();
        self.invalidate(invalidation);
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

        if let Some(pressed) = self.pointer.pressed()
            && !self.element_states.contains(pressed)
        {
            self.pointer.cancel();
        }
    }

    pub fn focus_next(&mut self) -> bool {
        let Some(root) = self.root else {
            let invalidation = self.set_focus(None);
            self.invalidate(invalidation);
            return false;
        };

        let Some(target) = self.frame.next_focus_target(root, self.focused) else {
            let invalidation = self.set_focus(None);
            self.invalidate(invalidation);
            return false;
        };

        let invalidation = self.set_focus(Some(target.element));
        self.invalidate(invalidation);

        true
    }

    pub fn focus_previous(&mut self) -> bool {
        let Some(root) = self.root else {
            let invalidation = self.set_focus(None);
            self.invalidate(invalidation);
            return false;
        };

        let Some(target) = self.frame.previous_focus_target(root, self.focused) else {
            let invalidation = self.set_focus(None);
            self.invalidate(invalidation);
            return false;
        };

        let invalidation = self.set_focus(Some(target.element));
        self.invalidate(invalidation);

        true
    }

    pub fn clear_focus(&mut self) {
        let invalidation = self.set_focus(None);
        self.invalidate(invalidation);
    }

    pub fn activate_focused(&mut self) -> Result<bool, ListenerInvokeError> {
        let Some(root) = self.root else {
            self.focused = None;
            return Ok(false);
        };
        let Some(focused) = self.focused else {
            return Ok(false);
        };
        let Some(target) = self.frame.click_target_for_element(root, focused) else {
            self.focused = None;
            return Ok(false);
        };

        let listener = Listener::from_id(target.listener);
        self.callbacks
            .invoke_listener(listener, &ClickEvent, &self.entities, &self.notified)?;

        Ok(true)
    }

    fn invalidate(&self, invalidation: Invalidation) {
        let current = self.visual_invalidation.get();
        self.visual_invalidation.set(current.merge(invalidation));
    }

    pub fn invalidation(&self) -> Invalidation {
        let visual = self.visual_invalidation.get();
        if self.notified.get() {
            visual.merge(Invalidation::Rebuild)
        } else {
            visual
        }
    }

    pub fn take_invalidation(&self) -> Invalidation {
        let visual = self.visual_invalidation.replace(Invalidation::None);
        let application = if self.notified.replace(false) {
            Invalidation::Rebuild
        } else {
            Invalidation::None
        };

        visual.merge(application)
    }

    fn refresh_interaction_styles(&mut self) {
        self.frame
            .resolve_interaction_styles(self.focused, self.pointer.pressed());

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
        self.invalidate(Invalidation::Paint);

        true
    }

    fn scroll_element_into_view_now(&mut self, element: ElementStateId) -> bool {
        let Some(root) = self.root else { return false };
        self.frame
            .scroll_element_into_view(root, element, &mut self.scroll_states)
    }

    fn set_focus(&mut self, next: Option<ElementStateId>) -> Invalidation {
        let previous = self.focused;
        if previous == next {
            if let Some(element) = next
                && self.scroll_element_into_view_now(element)
            {
                return Invalidation::Paint;
            }

            return Invalidation::None;
        }

        let mut invalidation = self.focus_transition_invalidation(previous, next);

        self.focused = next;
        self.refresh_interaction_styles();
        self.pending_scroll_into_view = None;

        let Some(element) = next else {
            return invalidation;
        };
        if matches!(invalidation, Invalidation::Layout | Invalidation::Rebuild) {
            // focus styling changed geometry. Current frame bounds are stale, so wait
            // until layout has recomputed them
            self.pending_scroll_into_view = Some(element);
        } else if self.scroll_element_into_view_now(element) {
            invalidation = invalidation.merge(Invalidation::Paint);
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
        let Some(root) = self.root else {
            return Ok(false);
        };
        let Some(focused) = self.focused else {
            return Ok(false);
        };
        let Some(node) = self.frame.node_for_element(root, focused) else {
            return Ok(false);
        };

        self.dispatch_to_node(node, event)
    }

    pub fn dispatch_at<E>(&self, position: Point, event: &E) -> Result<bool, ListenerInvokeError>
    where
        E: 'static,
    {
        let Some(root) = self.root else {
            return Ok(false);
        };

        let Some(node) = self.frame.hit_test_event(root, position, TypeId::of::<E>()) else {
            return Ok(false);
        };

        self.dispatch_to_node(node, event)
    }
}
