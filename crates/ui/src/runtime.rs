use core::cell::Cell;

use embedded_graphics::{draw_target::DrawTarget, pixelcolor::Rgb888};

use crate::{
    Context, Entity, EntityAllocError, EntityArena, FrameArena, Listener, MountError, NodeId,
    Render, Size, TextMeasurer,
    element_state::{ElementStateTable, IdentityError},
    entity_store::create_entity,
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
    frame_generation: u32,
    root: Option<NodeId>,
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
            frame_generation: 0,
            root: None,
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
        let generation = self.next_frame_generation();

        let result = self.build_frame(root, generation);

        match result {
            Ok(root_node) => {
                self.element_states.sweep(generation);
                self.root = Some(root_node);

                Ok(root_node)
            }
            Err(error) => {
                // identity resolition may have partially touched persistent state
                self.element_states.abort_frame(generation);
                // never expose a partial frame
                self.frame.clear();
                // never keep listeners registered by a failed render
                self.listeners.reset();
                self.root = None;

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
        self.notified.get()
    }

    pub fn take_dirty(&self) -> bool {
        self.notified.replace(false)
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

    fn paint<D, P>(&self, target: &mut D, text_painter: &P) -> Result<Option<()>, D::Error>
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
}
