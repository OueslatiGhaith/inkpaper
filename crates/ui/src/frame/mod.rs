use crate::{
    CanvasDraw, CanvasStyle, ElementId, EntityAccessError, EntityId, EntityRenderFn, EventBinding,
    EventBindingId, ImageSource, ImageStyle, Offset, Rect, ResolvedTextStyle, Size, Style,
    StylePatch, SvgSource, SvgStyle, TextStyle, element::state::ElementStateId,
    frame::storage::FrameBuffer, interaction::scroll::ScrollAxes, storage::FrameStorage,
};
#[cfg(feature = "metrics")]
use crate::{PerformanceMetrics, PerformanceMetricsCell};

mod mount;
mod rerender;
mod resolve;
mod storage;

#[cfg(all(test, feature = "alloc"))]
mod alloc_tests;
#[cfg(test)]
mod tests;

pub(crate) use mount::FrameStore;
pub use mount::MountCx;

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
    Svg {
        source: SvgSource,
        style: SvgStyle,
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
    pub(crate) text_style: TextStyle,
    pub(crate) effective_text_style: ResolvedTextStyle,
    pub(crate) first_event_binding: Option<EventBindingId>,
    pub(crate) last_event_binding: Option<EventBindingId>,
    /// left behind by a partial rebuild. Detached nodes keep their links, but are no
    /// longer in the tree, so passes over the whole arena skip them
    pub(crate) detached: bool,
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
            text_style: TextStyle::default(),
            effective_text_style: ResolvedTextStyle::default(),
            first_event_binding: None,
            last_event_binding: None,
            detached: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct NodeInteraction {
    pub(crate) focusable: bool,
    /// index into the frame's interaction styles, for nodes with focused or pressed styles
    pub(crate) styles: Option<u16>,
    pub(crate) scroll_axes: ScrollAxes,
    pub(crate) scroll_offset: Offset,
    pub(crate) initial_scroll_child: Option<usize>,
}

/// the focused and pressed styles of one node. Few nodes have them, so the frame keeps
/// them in their own table instead of in every [`Node`]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct InteractionStyles {
    pub(crate) focused: StylePatch,
    pub(crate) pressed: StylePatch,
}

impl InteractionStyles {
    fn is_empty(self) -> bool {
        self == Self::default()
    }

    fn merge(self, later: Self) -> Self {
        Self {
            focused: self.focused.merge(later.focused),
            pressed: self.pressed.merge(later.pressed),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MountError {
    NodesFull,
    TextStorageFull,
    EventBindingsFull,
    /// too many nodes have focused or pressed styles
    InteractionStylesFull,
    EntityAccess(EntityAccessError),
    DuplicateEntityMount(EntityId),
    /// heap-backed storage could not allocate
    AllocationFailed,
}

impl From<EntityAccessError> for MountError {
    fn from(value: EntityAccessError) -> Self {
        Self::EntityAccess(value)
    }
}

const fn entity_order(entity: EntityId) -> (u16, u16) {
    (entity.slot(), entity.generation())
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

pub(crate) struct FrameArena<F: FrameStorage> {
    pub(crate) nodes: FrameBuffer<Node, F::Nodes<Node>>,
    /// the node of each mounted entity, sorted by entity id
    entity_nodes: FrameBuffer<(EntityId, NodeId), F::Nodes<(EntityId, NodeId)>>,
    /// during a partial rebuild, the entity nodes cut from the tree that may be mounted
    /// again
    detached_entities: FrameBuffer<(EntityId, NodeId), F::Nodes<(EntityId, NodeId)>>,
    /// the entities that rendered or left the frame in the last rebuild
    stale_entities: FrameBuffer<EntityId, F::Nodes<EntityId>>,
    /// the entity nodes a partial rebuild rendered again, none inside another
    rerendered_nodes: FrameBuffer<NodeId, F::Nodes<NodeId>>,
    detached_nodes: usize,
    pub(crate) event_bindings: FrameBuffer<EventBinding, F::Nodes<EventBinding>>,
    text: FrameBuffer<u8, F::Text>,
    node_cache: FrameBuffer<NodeCache, F::Nodes<NodeCache>>,
    interaction_styles: FrameBuffer<InteractionStyles, F::InteractionStyles<InteractionStyles>>,
    /// the elements whose interaction styles apply
    focused: Option<ElementStateId>,
    pressed: Option<ElementStateId>,
    subtree_paint_bounds_valid: bool,
    #[cfg(feature = "metrics")]
    pub(crate) metrics: PerformanceMetricsCell,
}

impl<F: FrameStorage> Default for FrameArena<F> {
    fn default() -> Self {
        Self {
            nodes: FrameBuffer::new(),
            entity_nodes: FrameBuffer::new(),
            detached_entities: FrameBuffer::new(),
            stale_entities: FrameBuffer::new(),
            rerendered_nodes: FrameBuffer::new(),
            detached_nodes: 0,
            event_bindings: FrameBuffer::new(),
            text: FrameBuffer::new(),
            node_cache: FrameBuffer::new(),
            interaction_styles: FrameBuffer::new(),
            focused: None,
            pressed: None,
            subtree_paint_bounds_valid: false,
            #[cfg(feature = "metrics")]
            metrics: PerformanceMetricsCell::default(),
        }
    }
}

impl<F: FrameStorage> FrameArena<F> {
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn node_capacity(&self) -> usize {
        self.nodes.capacity().min(self.node_cache.capacity())
    }

    pub fn text_bytes_used(&self) -> usize {
        self.text.len()
    }

    pub fn text_capacity(&self) -> usize {
        self.text.capacity()
    }

    pub fn clear(&mut self) {
        self.nodes.clear();
        self.entity_nodes.clear();
        self.detached_entities.clear();
        self.stale_entities.clear();
        self.rerendered_nodes.clear();
        self.detached_nodes = 0;
        self.text.clear();
        self.event_bindings.clear();
        self.node_cache.clear();
        self.interaction_styles.clear();
        self.focused = None;
        self.pressed = None;
        self.subtree_paint_bounds_valid = false;
    }

    pub(crate) fn shrink_to_fit(&mut self) {
        self.nodes.shrink_to_fit();
        self.entity_nodes.shrink_to_fit();
        self.detached_entities.shrink_to_fit();
        self.stale_entities.shrink_to_fit();
        self.rerendered_nodes.shrink_to_fit();
        self.node_cache.shrink_to_fit();
        self.event_bindings.shrink_to_fit();
        self.text.shrink_to_fit();
        self.interaction_styles.shrink_to_fit();
    }

    pub(crate) fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id.index()]
    }

    pub(crate) fn node_mut(&mut self, id: NodeId) -> &mut Node {
        &mut self.nodes[id.index()]
    }

    /// the node an entity is mounted at, if it is in this frame
    pub(crate) fn entity_node(&self, entity: EntityId) -> Option<NodeId> {
        self.entity_node_position(entity)
            .ok()
            .map(|position| self.entity_nodes[position].1)
    }

    fn entity_node_position(&self, entity: EntityId) -> Result<usize, usize> {
        self.entity_nodes
            .binary_search_by_key(&entity_order(entity), |&(entity, _)| entity_order(entity))
    }

    /// a div's style, with its focused and pressed styles applied while its element is
    /// focused or pressed. `None` for other nodes
    pub(crate) fn style(&self, id: NodeId) -> Option<Style> {
        let node = self.node(id);
        let NodeKind::Div { style } = node.kind else {
            return None;
        };
        let (Some(index), Some(element)) = (node.interaction.styles, node.element_state_id) else {
            return Some(style);
        };

        let styles = &self.interaction_styles[usize::from(index)];
        let mut style = style;
        if self.focused == Some(element) {
            style = styles.focused.apply(style);
        }
        if self.pressed == Some(element) {
            style = styles.pressed.apply(style);
        }

        Some(style)
    }

    pub(crate) fn interaction_styles_for_element(
        &self,
        element: ElementStateId,
    ) -> Option<&InteractionStyles> {
        let node = self
            .nodes
            .iter()
            .find(|node| !node.detached && node.element_state_id == Some(element))?;

        node.interaction
            .styles
            .map(|index| &self.interaction_styles[usize::from(index)])
    }

    pub(crate) fn text(&self, range: TextRange) -> &str {
        let bytes = &self.text[range.start..range.start + range.len];

        // bytes were copied from a valid str
        unsafe { core::str::from_utf8_unchecked(bytes) }
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

    /// the node after `current` in tree order, without leaving the subtree under `root`
    pub(crate) fn next_depth_first_node_within(
        &self,
        current: NodeId,
        root: NodeId,
    ) -> Option<NodeId> {
        if let Some(child) = self.node(current).first_child {
            return Some(child);
        }

        self.next_node_after_subtree_within(current, root)
    }

    /// the node after `current`'s subtree in tree order, without leaving the subtree
    /// under `root`
    pub(crate) fn next_node_after_subtree_within(
        &self,
        current: NodeId,
        root: NodeId,
    ) -> Option<NodeId> {
        let mut node = current;
        while node != root {
            if let Some(sibling) = self.node(node).next_sibling {
                return Some(sibling);
            }

            node = self.node(node).parent?;
        }

        None
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
