use core::cell::Cell;

use embedded_graphics::{draw_target::DrawTarget, pixelcolor::Rgb888};

use crate::{
    ClickEvent, Context, Entity, EntityAllocError, EntityArena, FrameArena, Listener, MountError,
    NodeId, Point, Render, Size, TextMeasurer,
    element_state::{ElementStateId, ElementStateTable, IdentityError},
    entity_store::create_entity,
    input::PointerState,
    listener_store::{ListenerArena, ListenerInvokeError},
    paint::TextPainter,
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

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Invalidation {
    #[default]
    None,
    Paint,
    Layout,
    Rebuild,
}

impl Invalidation {
    pub const fn merge(self, other: Self) -> Self {
        use Invalidation::*;

        match (self, other) {
            (Rebuild, _) | (_, Rebuild) => Rebuild,
            (Layout, _) | (_, Layout) => Layout,
            (Paint, _) | (_, Paint) => Paint,
            _ => None,
        }
    }
}

pub struct Runtime<
    const ENTITY_BYTES: usize,
    const ENTITY_SLOTS: usize,
    const LISTENER_BYTES: usize,
    const LISTENER_SLOTS: usize,
    const FRAME_NODES: usize,
    const FRAME_TEXT_BYTES: usize,
    const ELEMENT_STATES: usize,
> {
    entities: EntityArena<ENTITY_BYTES, ENTITY_SLOTS>,
    listeners: ListenerArena<LISTENER_BYTES, LISTENER_SLOTS>,
    frame: FrameArena<FRAME_NODES, FRAME_TEXT_BYTES>,
    element_states: ElementStateTable<ELEMENT_STATES>,
    notified: Cell<bool>,
    visual_invalidation: Cell<Invalidation>,
    frame_generation: u32,
    root: Option<NodeId>,
    pointer: PointerState,
    focused: Option<ElementStateId>,
}

impl<
    const EB: usize,
    const ES: usize,
    const LB: usize,
    const LS: usize,
    const FN: usize,
    const FT: usize,
    const ST: usize,
> Default for Runtime<EB, ES, LB, LS, FN, FT, ST>
{
    fn default() -> Self {
        Self {
            entities: EntityArena::default(),
            listeners: ListenerArena::default(),
            frame: FrameArena::default(),
            element_states: ElementStateTable::default(),
            notified: Cell::new(false),
            visual_invalidation: Cell::new(Invalidation::None),
            frame_generation: 0,
            root: None,
            pointer: PointerState::default(),
            focused: None,
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
        create_entity(&self.entities, &self.listeners, &self.notified, build)
    }

    fn next_frame_generation(&mut self) -> u32 {
        let next = self.frame_generation.wrapping_add(1);

        if next == 0 {
            // `last_seen_frame` is i32, so don't allow old frame numbers to alias
            // after wrap around
            self.element_states.clear();
            self.frame_generation = 1;
        } else {
            self.frame_generation = next;
        }

        self.frame_generation
    }

    pub fn rebuild<T>(&mut self, root: Entity<T>) -> Result<NodeId, FrameBuildError>
    where
        T: Render,
    {
        self.root = None;
        self.frame.clear();

        // invalidate every ListenerId from the previous frame
        self.listeners.reset();
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

                Ok(root_node)
            }
            Err(error) => {
                // identity resolition may have partially touched persistent state
                self.element_states.abort_frame(generation);
                // never expose a partial frame
                self.frame.clear();
                // never keep listeners registered by a failed render
                self.listeners.reset();
                self.pointer.cancel();
                self.visual_invalidation.set(Invalidation::None);
                self.root = None;
                self.focused = None;

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
        let root_node = self.frame.mount(root)?;

        self.frame
            .expand_entities(&self.entities, &self.listeners, &self.notified)?;

        self.frame
            .resolve_identities(&mut self.element_states, generation)?;

        Ok(root_node)
    }

    pub fn root_node(&self) -> Option<NodeId> {
        self.root
    }

    pub fn frame(&self) -> &FrameArena<FN, FT> {
        &self.frame
    }

    pub fn frame_node_count(&self) -> usize {
        self.frame.node_count()
    }

    pub fn frame_text_bytes_used(&self) -> usize {
        self.frame.text_bytes_used()
    }

    pub fn is_dirty(&self) -> bool {
        self.invalidation() != Invalidation::None
    }

    pub fn take_dirty(&self) -> bool {
        self.take_invalidation() != Invalidation::None
    }

    pub fn invoke<E>(&self, listener: Listener<E>, event: &E) -> Result<(), ListenerInvokeError>
    where
        E: 'static,
    {
        self.listeners
            .invoke(listener, event, &self.entities, &self.notified)
    }

    pub fn layout(&mut self, viewport: Size, text_measurer: &dyn TextMeasurer) -> Option<Size> {
        let root = self.root?;

        Some(self.frame.layout(root, viewport, text_measurer))
    }

    pub fn paint<D, P>(&self, target: &mut D, text_painter: &P) -> Result<Option<()>, D::Error>
    where
        D: DrawTarget<Color = Rgb888>,
        D::Color: From<Rgb888>,
        P: TextPainter,
    {
        let Some(root) = self.root else {
            return Ok(None);
        };

        self.frame.paint(root, target, text_painter)?;

        Ok(Some(()))
    }

    pub fn pointer_down(&mut self, position: Point) -> bool {
        let Some(root) = self.root else {
            if self.pointer.pressed().is_some() {
                self.pointer.cancel();
                self.invalidate(Invalidation::Layout);
            }

            return false;
        };

        let target = self.frame.hit_test_click(root, position);
        let pressed = target.map(|target| target.element);
        let changed = self.pointer.pressed() != pressed;
        self.pointer.press(pressed);
        if changed {
            self.refresh_interaction_styles();
            self.invalidate(Invalidation::Layout);
        }

        target.is_some()
    }

    pub fn pointer_up(&mut self, position: Point) -> Result<bool, ListenerInvokeError> {
        let Some(pressed) = self.pointer.take_pressed() else {
            return Ok(false);
        };

        let mut listener = None;
        let mut activated = false;

        if let Some(root) = self.root
            && let Some(target) = self.frame.hit_test_click(root, position)
            && target.element == pressed
        {
            self.focused = Some(target.element);
            listener = Some(target.listener);
            activated = true;
        };

        self.refresh_interaction_styles();
        self.invalidate(Invalidation::Layout);

        if let Some(listener) = listener {
            let listener = Listener::from_id(listener);
            self.listeners
                .invoke(listener, &ClickEvent, &self.entities, &self.notified)?;
        }

        Ok(activated)
    }

    pub fn pointer_cancel(&mut self) {
        if self.pointer.pressed().is_none() {
            return;
        }

        self.pointer.cancel();
        self.refresh_interaction_styles();
        self.invalidate(Invalidation::Layout);
    }

    fn reconcile_interaction_state(&mut self) {
        if let Some(focused) = self.focused
            && !self.element_states.contains(focused)
        {
            self.focused = None;
        }

        if let Some(pressed) = self.pointer.pressed()
            && !self.element_states.contains(pressed)
        {
            self.pointer.cancel();
        }
    }

    pub fn focus_next(&mut self) -> bool {
        let Some(root) = self.root else {
            self.focused = None;
            return false;
        };
        let Some(target) = self.frame.next_click_traget(root, self.focused) else {
            self.focused = None;
            return false;
        };

        let next = Some(target.element);
        if self.focused != next {
            self.focused = next;
            self.refresh_interaction_styles();
            self.invalidate(Invalidation::Layout);
        }

        true
    }

    pub fn focus_previous(&mut self) -> bool {
        let Some(root) = self.root else {
            self.focused = None;
            return false;
        };
        let Some(target) = self.frame.previous_click_target(root, self.focused) else {
            self.focused = None;
            return false;
        };

        let previous = Some(target.element);
        if self.focused != previous {
            self.focused = previous;
            self.refresh_interaction_styles();
            self.invalidate(Invalidation::Layout);
        }

        true
    }

    pub fn clear_focus(&mut self) {
        if self.focused.is_none() {
            return;
        }

        self.focused = None;
        self.refresh_interaction_styles();
        self.invalidate(Invalidation::Layout);
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
        self.listeners
            .invoke(listener, &ClickEvent, &self.entities, &self.notified)?;

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
    }
}
