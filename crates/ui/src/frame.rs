use core::{any::TypeId, cell::Cell};

use heapless::Vec;

use crate::{
    AppContext, CanvasDraw, CanvasStyle, Element, ElementId, EntityAccessError, EntityId,
    EntityRenderFn, EventBinding, EventBindingId, EventCallbacks, ImageSource, ImageStyle,
    IntoElement, Offset, Rect, ResolvedTextStyle, Size, StatefulInteractivity, Style, StylePatch,
    TextStyle,
    callback::CallbackId,
    callback::CallbackStore,
    count_metric,
    element_state::{ElementStateId, ElementStateTable, IdentityError, IdentityParent},
    entity::EntityStore,
    global::GlobalStore,
    scroll::{ScrollAxes, ScrollStateTable},
};
#[cfg(feature = "metrics")]
use crate::{PerformanceMetrics, PerformanceMetricsCell};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(u16);

impl NodeId {
    pub(crate) const fn new(index: u16) -> Self {
        Self(index)
    }

    pub(crate) const fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TextRange {
    start: usize,
    len: usize,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum NodeKind {
    Div {
        style: Style,
    },
    Text {
        text: TextRange,
    },
    Image {
        source: ImageSource,
        style: ImageStyle,
    },
    Canvas {
        draw: CanvasDraw,
        style: CanvasStyle,
    },
    Entity {
        entity: EntityId,
        render: EntityRenderFn,
        expanded: bool,
    },
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NodeLayout {
    pub(crate) bounds: Rect,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Node {
    pub(crate) kind: NodeKind,
    pub(crate) parent: Option<NodeId>,
    pub(crate) first_child: Option<NodeId>,
    pub(crate) last_child: Option<NodeId>,
    pub(crate) next_sibling: Option<NodeId>,
    pub(crate) element_id: Option<ElementId>,
    pub(crate) element_state_id: Option<ElementStateId>,
    pub(crate) interaction: NodeInteraction,
    pub(crate) layout: NodeLayout,
    pub(crate) effective_style: Option<Style>,
    pub(crate) text_style: TextStyle,
    pub(crate) effective_text_style: ResolvedTextStyle,
    pub(crate) first_event_binding: Option<EventBindingId>,
    pub(crate) last_event_binding: Option<EventBindingId>,
}

impl Node {
    fn new(kind: NodeKind) -> Self {
        Self {
            kind,
            parent: None,
            first_child: None,
            last_child: None,
            next_sibling: None,
            element_id: None,
            element_state_id: None,
            interaction: NodeInteraction::default(),
            layout: NodeLayout::default(),
            effective_style: match kind {
                NodeKind::Div { style } => Some(style),
                _ => None,
            },
            text_style: TextStyle::default(),
            effective_text_style: ResolvedTextStyle::default(),
            first_event_binding: None,
            last_event_binding: None,
        }
    }

    pub(crate) fn style(&self) -> Option<Style> {
        match self.kind {
            NodeKind::Div { style } => Some(self.effective_style.unwrap_or(style)),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct NodeInteraction {
    pub(crate) focusable: bool,
    pub(crate) focused_style: StylePatch,
    pub(crate) pressed_style: StylePatch,
    pub(crate) scroll_axes: ScrollAxes,
    pub(crate) scroll_offset: Offset,
}

impl NodeInteraction {
    fn apply(&mut self, interaction: StatefulInteractivity) {
        self.focusable |= interaction.focusable;
        self.focused_style = self.focused_style.merge(interaction.focused_style);
        self.pressed_style = self.pressed_style.merge(interaction.pressed_style);
        if interaction.scroll_axes.any() {
            self.scroll_axes = interaction.scroll_axes;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MountError {
    NodesFull,
    TextStorageFull,
    EventBindingsFull,
    EntityAccess(EntityAccessError),
    DuplicateEntityMount(EntityId),
}

impl From<EntityAccessError> for MountError {
    fn from(value: EntityAccessError) -> Self {
        Self::EntityAccess(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MeasurementCache {
    available: Size,
    measured: Size,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NodeCache {
    Empty,
    Measurement(MeasurementCache),
    SubtreePaintBounds(Rect),
}

pub(crate) struct FrameArena<const NODES: usize, const TEXT_BYTES: usize> {
    pub(crate) nodes: Vec<Node, NODES>,
    pub(crate) event_bindings: Vec<EventBinding, NODES>,
    text: Vec<u8, TEXT_BYTES>,
    node_cache: [NodeCache; NODES],
    subtree_paint_bounds_valid: bool,
    #[cfg(feature = "metrics")]
    pub(crate) metrics: PerformanceMetricsCell,
}

impl<const NODES: usize, const TEXT_BYTES: usize> Default for FrameArena<NODES, TEXT_BYTES> {
    fn default() -> Self {
        Self {
            nodes: Vec::new(),
            event_bindings: Vec::new(),
            text: Vec::new(),
            node_cache: [NodeCache::Empty; NODES],
            subtree_paint_bounds_valid: false,
            #[cfg(feature = "metrics")]
            metrics: PerformanceMetricsCell::default(),
        }
    }
}

impl<const NODES: usize, const TEXT_BYTES: usize> FrameArena<NODES, TEXT_BYTES> {
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub const fn node_capacity(&self) -> usize {
        NODES
    }

    pub fn text_bytes_used(&self) -> usize {
        self.text.len()
    }

    pub const fn text_capacity(&self) -> usize {
        TEXT_BYTES
    }

    pub fn clear(&mut self) {
        self.nodes.clear();
        self.text.clear();
        self.event_bindings.clear();
        self.subtree_paint_bounds_valid = false;
    }

    fn push_node(&mut self, kind: NodeKind) -> Result<NodeId, MountError> {
        if self.nodes.len() >= NODES || self.nodes.len() > u16::MAX as usize {
            return Err(MountError::NodesFull);
        }

        let id = NodeId::new(self.nodes.len() as u16);
        self.nodes
            .push(Node::new(kind))
            .map_err(|_| MountError::NodesFull)?;

        self.subtree_paint_bounds_valid = false;

        count_metric!(self, nodes_mounted);

        Ok(id)
    }

    pub fn mount<E>(&mut self, element: E, app: AppContext<'_>) -> Result<NodeId, MountError>
    where
        E: IntoElement,
    {
        let element = element.into_element();
        let mut cx = MountCx::new(self, app);

        element.mount(&mut cx)
    }

    pub(crate) fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id.index()]
    }

    pub(crate) fn node_mut(&mut self, id: NodeId) -> &mut Node {
        &mut self.nodes[id.index()]
    }

    pub(crate) fn text(&self, range: TextRange) -> &str {
        let bytes = &self.text[range.start..range.start + range.len];

        // bytes were copied from a valid str
        unsafe { core::str::from_utf8_unchecked(bytes) }
    }

    fn contains_entity(&self, entity: EntityId) -> bool {
        self.nodes.iter().any(|node| {
            matches!(
                node.kind,
                NodeKind::Entity {
                    entity: existing,
                    ..
                } if existing == entity
            )
        })
    }

    pub(crate) fn expand_entities(
        &mut self,
        entities: &dyn EntityStore,
        globals: &dyn GlobalStore,
        callbacks: &dyn CallbackStore,
        notified: &Cell<bool>,
    ) -> Result<(), MountError> {
        let mut index = 0;

        while index < self.nodes.len() {
            let pending = match self.nodes[index].kind {
                NodeKind::Entity {
                    entity,
                    render,
                    expanded: false,
                } => Some((entity, render)),
                _ => None,
            };

            if let Some((entity, render)) = pending {
                count_metric!(self, entity_render_calls);
                let entity_node = NodeId::new(index as u16);
                let root = render(entity, entities, globals, callbacks, notified, self)?;
                self.append_child(entity_node, root);

                match &mut self.nodes[index].kind {
                    NodeKind::Entity { expanded, .. } => *expanded = true,
                    _ => unreachable!(),
                }
            }

            index += 1;
        }

        Ok(())
    }

    pub(crate) fn mount_and_expand<E>(
        &mut self,
        element: E,
        entities: &dyn EntityStore,
        globals: &dyn GlobalStore,
        callbacks: &dyn CallbackStore,
        notified: &Cell<bool>,
    ) -> Result<NodeId, MountError>
    where
        E: IntoElement,
    {
        let app = AppContext::from_globals(globals);
        let root = self.mount(element, app)?;
        self.expand_entities(entities, globals, callbacks, notified)?;

        Ok(root)
    }

    fn identity_parent(&self, node: NodeId) -> Option<IdentityParent> {
        let mut current = self.nodes[node.index()].parent;

        while let Some(parent_id) = current {
            let parent = &self.nodes[parent_id.index()];

            // entity in an identity boundary
            if let NodeKind::Entity { entity, .. } = parent.kind {
                return Some(IdentityParent::Entity(entity));
            }
            // otherwise the nearest identified ancestor establishes the scope
            if let Some(state_id) = parent.element_state_id {
                return Some(IdentityParent::Element(state_id));
            }
            // unnamed elements do not contribute
            current = parent.parent;
        }

        None
    }

    pub(crate) fn resolve_identities<const STATES: usize>(
        &mut self,
        states: &mut ElementStateTable<STATES>,
        frame_generation: u32,
    ) -> Result<(), IdentityError> {
        let len = self.nodes.len();

        for index in 0..len {
            let node_id = NodeId::new(index as u16);
            let Some(local_id) = self.nodes[index].element_id else {
                continue;
            };

            let parent = self
                .identity_parent(node_id)
                .ok_or(IdentityError::MissingEntityScope { node: node_id })?;

            let state_id = states.resolve(parent, local_id, frame_generation)?;

            self.nodes[index].element_state_id = Some(state_id);
        }

        Ok(())
    }

    pub fn bounds(&self, id: NodeId) -> Rect {
        self.node(id).layout.bounds
    }

    pub(crate) fn next_depth_first_node(&self, current: NodeId) -> Option<NodeId> {
        if let Some(child) = self.node(current).first_child {
            return Some(child);
        }

        let mut node = current;
        loop {
            if let Some(sibling) = self.node(node).next_sibling {
                return Some(sibling);
            }

            let parent = self.node(node).parent?;
            node = parent
        }
    }

    pub(crate) fn resolve_interaction_styles(
        &mut self,
        focused: Option<ElementStateId>,
        pressed: Option<ElementStateId>,
    ) {
        for node in self.nodes.iter_mut() {
            let NodeKind::Div { style: base } = node.kind else {
                node.effective_style = None;
                continue;
            };

            let mut effective = base;
            if node.element_state_id == focused {
                effective = node.interaction.focused_style.apply(effective);
            }
            if node.element_state_id == pressed {
                effective = node.interaction.pressed_style.apply(effective);
            }

            node.effective_style = Some(effective);
        }
    }

    pub(crate) fn resolve_scroll_offsets<const SLOTS: usize>(
        &mut self,
        states: &ScrollStateTable<SLOTS>,
    ) {
        for node in self.nodes.iter_mut() {
            node.interaction.scroll_offset = node
                .element_state_id
                .map(|id| states.offset(id))
                .unwrap_or(Offset::ZERO);
        }
    }

    pub(crate) fn resolve_text_styles(&mut self, root: NodeId) {
        let mut current = Some(root);
        while let Some(node_id) = current {
            let inherited = self
                .node(node_id)
                .parent
                .map(|parent| self.node(parent).effective_text_style)
                .unwrap_or_default();

            let resolved = match self.node(node_id).kind {
                NodeKind::Div { .. } => self
                    .node(node_id)
                    .style()
                    .expect("div node must have style")
                    .text
                    .resolve(inherited),
                NodeKind::Text { .. } => self.node(node_id).text_style.resolve(inherited),
                NodeKind::Image { .. } | NodeKind::Entity { .. } | NodeKind::Canvas { .. } => {
                    inherited
                }
            };

            self.node_mut(node_id).effective_text_style = resolved;
            current = self.next_depth_first_node(node_id);
        }
    }

    pub(crate) fn bind_event(
        &mut self,
        node: NodeId,
        event_type: TypeId,
        callback: CallbackId,
    ) -> Result<(), MountError> {
        if self.event_bindings.len() >= NODES || self.event_bindings.len() > u16::MAX as usize {
            return Err(MountError::EventBindingsFull);
        }

        let binding_id = EventBindingId::new(self.event_bindings.len() as u16);
        self.event_bindings
            .push(EventBinding {
                event_type,
                callback,
                next: None,
            })
            .map_err(|_| MountError::EventBindingsFull)?;

        let node_index = node.index();

        match self.nodes[node_index].last_event_binding {
            Some(previous) => self.event_bindings[previous.index()].next = Some(binding_id),
            None => self.nodes[node_index].first_event_binding = Some(binding_id),
        }

        self.nodes[node_index].last_event_binding = Some(binding_id);

        Ok(())
    }

    pub(crate) fn event_callbacks(&self, node: NodeId, event_type: TypeId) -> EventCallbacks<'_> {
        EventCallbacks::new(
            &self.event_bindings.as_slice(),
            self.node(node).first_event_binding,
            event_type,
        )
    }

    #[cfg(feature = "metrics")]
    pub(crate) fn reset_performance_metrics(&self) {
        self.metrics.reset();
    }

    #[cfg(feature = "metrics")]
    pub(crate) fn performance_metrics(&self) -> PerformanceMetrics {
        self.metrics.snapshot()
    }

    pub(crate) fn cached_measurement(&self, node: NodeId, available: Size) -> Option<Size> {
        match self.node_cache[node.index()] {
            NodeCache::Measurement(cache) if cache.available == available => Some(cache.measured),
            _ => None,
        }
    }

    pub(crate) fn cache_measurement(&mut self, node: NodeId, available: Size, measured: Size) {
        self.node_cache[node.index()] = NodeCache::Measurement(MeasurementCache {
            available,
            measured,
        });
    }

    pub(crate) fn clear_measurement_caches(&mut self) {
        let node_count = self.nodes.len();
        for cache in &mut self.node_cache[..node_count] {
            *cache = NodeCache::Empty;
        }
        self.subtree_paint_bounds_valid = false;
    }

    pub(crate) fn set_subtree_paint_bounds(&mut self, node: NodeId, bounds: Rect) {
        self.node_cache[node.index()] = NodeCache::SubtreePaintBounds(bounds);
    }

    pub(crate) fn subtree_paint_bounds(&self, node: NodeId) -> Option<Rect> {
        if !self.subtree_paint_bounds_valid {
            return None;
        }

        match self.node_cache[node.index()] {
            NodeCache::SubtreePaintBounds(bounds) => Some(bounds),
            NodeCache::Empty | NodeCache::Measurement(_) => None,
        }
    }

    pub(crate) fn finish_subtree_paint_bounds(&mut self) {
        self.subtree_paint_bounds_valid = true;
    }

    pub(crate) fn ordered_prefix_paint_bounds(&self, node: NodeId) -> Option<Rect> {
        // during layout, every non-last child of a `Div` receives the cumulative
        // paint extent of the sibling prefix ending at that child.
        // the last child deliberately keeps its exact subtree extent, so it is not
        // a prefix-search entry
        self.node(node).next_sibling?;
        self.subtree_paint_bounds(node)
    }
}

impl<const NODES: usize, const TEXT_BYTES: usize> FrameStore for FrameArena<NODES, TEXT_BYTES> {
    fn push_div(&mut self, style: Style) -> Result<NodeId, MountError> {
        self.push_node(NodeKind::Div { style })
    }

    fn push_text(&mut self, text: &str, style: TextStyle) -> Result<NodeId, MountError> {
        let bytes = text.as_bytes();
        let start = self.text.len();
        let end = start
            .checked_add(bytes.len())
            .ok_or(MountError::TextStorageFull)?;

        if end > TEXT_BYTES {
            return Err(MountError::TextStorageFull);
        }

        for byte in bytes {
            self.text
                .push(*byte)
                .map_err(|_| MountError::TextStorageFull)?;
        }

        let range = TextRange {
            start,
            len: bytes.len(),
        };
        let node = self.push_node(NodeKind::Text { text: range })?;
        self.node_mut(node).text_style = style;

        Ok(node)
    }

    fn push_image(&mut self, source: ImageSource, style: ImageStyle) -> Result<NodeId, MountError> {
        self.push_node(NodeKind::Image { source, style })
    }

    fn push_canvas(&mut self, draw: CanvasDraw, style: CanvasStyle) -> Result<NodeId, MountError> {
        self.push_node(NodeKind::Canvas { draw, style })
    }

    fn push_entity(
        &mut self,
        entity: EntityId,
        render: EntityRenderFn,
    ) -> Result<NodeId, MountError> {
        if self.contains_entity(entity) {
            return Err(MountError::DuplicateEntityMount(entity));
        }

        self.push_node(NodeKind::Entity {
            entity,
            render,
            expanded: false,
        })
    }

    fn append_child(&mut self, parent: NodeId, child: NodeId) {
        let parent_idx = parent.index();
        let child_idx = child.index();
        debug_assert!(
            child_idx > parent_idx,
            "frame children must be mounted after their parents"
        );

        self.nodes[child_idx].parent = Some(parent);

        match self.nodes[parent_idx].last_child {
            Some(last) => {
                debug_assert!(
                    child_idx > last.index(),
                    "frame siblings must be mounted in append order"
                );
                self.nodes[last.index()].next_sibling = Some(child)
            }
            None => self.nodes[parent_idx].first_child = Some(child),
        }

        self.nodes[parent_idx].last_child = Some(child);
    }

    fn identify(&mut self, node: NodeId, id: ElementId) {
        self.nodes[node.index()].element_id = Some(id);
    }

    fn apply_interactivity(&mut self, node: NodeId, interactivity: StatefulInteractivity) {
        self.nodes[node.index()].interaction.apply(interactivity);
    }

    fn bind_event(
        &mut self,
        node: NodeId,
        event_type: TypeId,
        callback: CallbackId,
    ) -> Result<(), MountError> {
        FrameArena::bind_event(self, node, event_type, callback)
    }
}

pub(crate) trait FrameStore {
    fn push_div(&mut self, style: Style) -> Result<NodeId, MountError>;
    fn push_text(&mut self, text: &str, style: TextStyle) -> Result<NodeId, MountError>;
    fn push_image(&mut self, source: ImageSource, style: ImageStyle) -> Result<NodeId, MountError>;
    fn push_canvas(&mut self, draw: CanvasDraw, style: CanvasStyle) -> Result<NodeId, MountError>;
    fn push_entity(
        &mut self,
        entity: EntityId,
        render: EntityRenderFn,
    ) -> Result<NodeId, MountError>;
    fn append_child(&mut self, parent: NodeId, child: NodeId);
    fn identify(&mut self, node: NodeId, id: ElementId);
    fn apply_interactivity(&mut self, node: NodeId, interactivity: StatefulInteractivity);
    fn bind_event(
        &mut self,
        node: NodeId,
        event_type: TypeId,
        callback: CallbackId,
    ) -> Result<(), MountError>;
}

pub struct MountCx<'a> {
    frame: &'a mut dyn FrameStore,
    app: AppContext<'a>,
}

impl<'a> MountCx<'a> {
    pub(crate) fn new(frame: &'a mut dyn FrameStore, app: AppContext<'a>) -> Self {
        Self { frame, app }
    }

    pub(crate) fn app_context(&self) -> AppContext<'a> {
        self.app
    }
}

impl MountCx<'_> {
    pub(crate) fn push_div(&mut self, style: Style) -> Result<NodeId, MountError> {
        self.frame.push_div(style)
    }

    pub(crate) fn push_text(&mut self, text: &str, style: TextStyle) -> Result<NodeId, MountError> {
        self.frame.push_text(text, style)
    }

    pub(crate) fn push_image(
        &mut self,
        source: ImageSource,
        style: ImageStyle,
    ) -> Result<NodeId, MountError> {
        self.frame.push_image(source, style)
    }

    pub(crate) fn push_canvas(
        &mut self,
        draw: CanvasDraw,
        style: CanvasStyle,
    ) -> Result<NodeId, MountError> {
        self.frame.push_canvas(draw, style)
    }

    pub(crate) fn push_entity(
        &mut self,
        entity: EntityId,
        render: EntityRenderFn,
    ) -> Result<NodeId, MountError> {
        self.frame.push_entity(entity, render)
    }

    pub(crate) fn append_child(&mut self, parent: NodeId, child: NodeId) {
        self.frame.append_child(parent, child);
    }

    pub(crate) fn identify(&mut self, node: NodeId, id: ElementId) {
        self.frame.identify(node, id);
    }

    pub(crate) fn apply_interactivity(
        &mut self,
        node: NodeId,
        interactivity: StatefulInteractivity,
    ) {
        self.frame.apply_interactivity(node, interactivity);
    }

    pub(crate) fn bind_event(
        &mut self,
        node: NodeId,
        event_type: TypeId,
        callback: CallbackId,
    ) -> Result<(), MountError> {
        self.frame.bind_event(node, event_type, callback)
    }
}

#[cfg(test)]
mod tests;
