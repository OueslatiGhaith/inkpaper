use crate::{
    CanvasDraw, CanvasStyle, DamageRegion, ElementId, EntityAccessError, EntityId, EntityRenderFn,
    EventBinding, ImageSource, ImageStyle, Offset, Rect, ResolvedTextStyle, Size, Style,
    StylePatch, SvgSource, SvgStyle, TextStyle,
    element::state::ElementStateId,
    frame::storage::FrameBuffer,
    interaction::scroll::ScrollAxes,
    layout::AvailableSize,
    pool::{OutOfMemory, PoolRef, Table},
};
#[cfg(feature = "metrics")]
use crate::{PerformanceMetrics, PerformanceMetricsCell};

mod compact;
mod mount;
mod rerender;
mod resolve;
mod storage;

#[cfg(test)]
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
        /// where the div's style sits in the frame's div styles
        style: u16,
    },
    Text {
        text: TextRange,
        /// where the text's style sits in the frame's text styles
        style: u16,
    },
    Image {
        source: ImageSource,
        style: ImageStyle,
    },
    Svg {
        source: SvgSource,
        style: SvgStyle,
        /// where the svg's text style sits in the frame's text styles
        text_style: u16,
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
    /// where the node's identity and interaction sit in the frame's node extras, for
    /// the few nodes that have them
    pub(crate) extras: Option<u16>,
    pub(crate) layout: NodeLayout,
    /// where the node's resolved text style sits in the frame's resolved text styles
    pub(crate) resolved_text_style: u16,
    pub(crate) event_bindings: BindingRun,
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
            extras: None,
            layout: NodeLayout::default(),
            resolved_text_style: 0,
            event_bindings: BindingRun::EMPTY,
            detached: false,
        }
    }
}

/// where a node's event bindings sit in the frame's binding table
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BindingRun {
    start: u16,
    len: u16,
}

impl BindingRun {
    const EMPTY: Self = Self { start: 0, len: 0 };

    fn range(self) -> core::ops::Range<usize> {
        let start = usize::from(self.start);

        start..start + usize::from(self.len)
    }
}

/// the identity and interaction of a node that has an element id or is interactive.
/// Few nodes do, so the frame keeps them in their own table instead of in every [`Node`]
#[derive(Debug, Clone, Copy)]
pub(crate) struct NodeExtras {
    /// the node these belong to
    owner: NodeId,
    pub(crate) element_id: Option<ElementId>,
    pub(crate) element_state_id: Option<ElementStateId>,
    pub(crate) interaction: NodeInteraction,
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
    /// every node id is taken
    NodesFull,
    /// every event binding id is taken
    EventBindingsFull,
    /// every interaction style id is taken
    InteractionStylesFull,
    EntityAccess(EntityAccessError),
    DuplicateEntityMount(EntityId),
    OutOfMemory(OutOfMemory),
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
    available: AvailableSize,
    measured: Size,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NodeCache {
    Empty,
    Measurement(MeasurementCache),
    SubtreePaintBounds(Rect),
    /// while the frame compacts, where a live node moves
    MovedTo(NodeId),
}

pub(crate) struct FrameArena {
    pub(crate) nodes: FrameBuffer<Node>,
    /// the node of each mounted entity, sorted by entity id
    entity_nodes: FrameBuffer<(EntityId, NodeId)>,
    /// during a partial rebuild, the entity nodes cut from the tree that may be mounted
    /// again
    detached_entities: FrameBuffer<(EntityId, NodeId)>,
    /// the entities that rendered or left the frame in the last rebuild
    stale_entities: FrameBuffer<EntityId>,
    /// the entity nodes a partial rebuild rendered again, none inside another
    rerendered_nodes: FrameBuffer<NodeId>,
    detached_nodes: usize,
    /// what the re-rendered subtrees painted before the last partial rebuild, while the
    /// layout after it is pending. `None` when unknown
    rerendered_damage: Option<DamageRegion>,
    /// while the layout after a partial rebuild is pending, the first node it mounted.
    /// Earlier nodes were laid out before
    tracked_layout_start: Option<usize>,
    /// a node outside the re-rendered subtrees moved or scrolled since the partial
    /// rebuild
    layout_changed_outside: bool,
    pub(crate) event_bindings: FrameBuffer<EventBinding>,
    text: FrameBuffer<u8>,
    /// the style of each div, in mount order, kept apart from the nodes since they're
    /// most of a node's size
    styles: FrameBuffer<Style>,
    /// the text style of each text and svg node, in mount order
    text_styles: FrameBuffer<TextStyle>,
    /// each text style the last resolve pass found, once. Nodes point into it
    resolved_text_styles: FrameBuffer<ResolvedTextStyle>,
    /// the nodes that can change the text style they inherit. A resolve pass finds at
    /// most one new style at each, and the default, so mounting keeps room for that many
    /// and resolving never allocates
    text_style_changes: usize,
    node_cache: FrameBuffer<NodeCache>,
    /// each with the node it belongs to
    interaction_styles: FrameBuffer<(NodeId, InteractionStyles)>,
    /// the identity and interaction of the nodes that have them, each with its node
    extras: FrameBuffer<NodeExtras>,
    /// the elements whose interaction styles apply
    focused: Option<ElementStateId>,
    pressed: Option<ElementStateId>,
    subtree_paint_bounds_valid: bool,
    #[cfg(feature = "metrics")]
    pub(crate) metrics: PerformanceMetricsCell,
}

impl FrameArena {
    /// an empty frame whose tables grow in `pool`
    pub(crate) fn new_in(pool: PoolRef) -> Self {
        Self {
            nodes: FrameBuffer::new_in(pool, Table::Nodes),
            entity_nodes: FrameBuffer::new_in(pool, Table::Nodes),
            detached_entities: FrameBuffer::new_in(pool, Table::Nodes),
            stale_entities: FrameBuffer::new_in(pool, Table::Nodes),
            rerendered_nodes: FrameBuffer::new_in(pool, Table::Nodes),
            detached_nodes: 0,
            rerendered_damage: None,
            tracked_layout_start: None,
            layout_changed_outside: false,
            event_bindings: FrameBuffer::new_in(pool, Table::Nodes),
            text: FrameBuffer::new_in(pool, Table::Text),
            styles: FrameBuffer::new_in(pool, Table::Nodes),
            text_styles: FrameBuffer::new_in(pool, Table::Nodes),
            resolved_text_styles: FrameBuffer::new_in(pool, Table::Nodes),
            text_style_changes: 0,
            node_cache: FrameBuffer::new_in(pool, Table::Nodes),
            interaction_styles: FrameBuffer::new_in(pool, Table::Nodes),
            extras: FrameBuffer::new_in(pool, Table::Nodes),
            focused: None,
            pressed: None,
            subtree_paint_bounds_valid: false,
            #[cfg(feature = "metrics")]
            metrics: PerformanceMetricsCell::default(),
        }
    }
}

impl FrameArena {
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    #[cfg(test)]
    pub fn node_capacity(&self) -> usize {
        self.nodes.capacity().min(self.node_cache.capacity())
    }

    pub fn text_bytes_used(&self) -> usize {
        self.text.len()
    }

    #[cfg(test)]
    pub(crate) fn div_style_count(&self) -> usize {
        self.styles.len()
    }

    #[cfg(test)]
    pub(crate) fn extras_count(&self) -> usize {
        self.extras.len()
    }

    #[cfg(test)]
    pub(crate) fn text_style_count(&self) -> usize {
        self.text_styles.len()
    }

    #[cfg(test)]
    pub(crate) fn resolved_text_style_count(&self) -> usize {
        self.resolved_text_styles.len()
    }

    #[cfg(test)]
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
        self.rerendered_damage = None;
        self.tracked_layout_start = None;
        self.layout_changed_outside = false;
        self.text.clear();
        self.styles.clear();
        self.text_styles.clear();
        self.resolved_text_styles.clear();
        self.text_style_changes = 0;
        self.event_bindings.clear();
        self.node_cache.clear();
        self.interaction_styles.clear();
        self.extras.clear();
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
        self.styles.shrink_to_fit();
        self.text_styles.shrink_to_fit();
        // the resolved styles keep the room mounting reserved, so resolving never
        // allocates
        self.interaction_styles.shrink_to_fit();
        self.extras.shrink_to_fit();
    }

    /// every node in the frame, detached ones included. Doesn't borrow the frame
    pub(crate) fn node_ids(&self) -> impl Iterator<Item = NodeId> + use<> {
        // `push_node` never indexes a node past u16
        (0..=u16::MAX).take(self.nodes.len()).map(NodeId::new)
    }

    pub(crate) fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id.index()]
    }

    pub(crate) fn node_mut(&mut self, id: NodeId) -> &mut Node {
        &mut self.nodes[id.index()]
    }

    #[cfg(test)]
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
        let style = self.styles[usize::from(style)];
        let (Some(index), Some(element)) = (self.interaction(id).styles, self.element_state_id(id))
        else {
            return Some(style);
        };

        let (_, styles) = &self.interaction_styles[usize::from(index)];
        let mut style = style;
        if self.focused == Some(element) {
            style = styles.focused.apply(style);
        }
        if self.pressed == Some(element) {
            style = styles.pressed.apply(style);
        }

        Some(style)
    }

    /// the text style the last resolve pass found for `id`
    pub(crate) fn resolved_text_style(&self, id: NodeId) -> ResolvedTextStyle {
        let index = usize::from(self.node(id).resolved_text_style);

        // before the first pass, nodes have the default
        self.resolved_text_styles
            .get(index)
            .copied()
            .unwrap_or_default()
    }

    /// the text style a text or svg node was mounted with
    #[cfg(test)]
    pub(crate) fn text_style_patch(&self, id: NodeId) -> Option<TextStyle> {
        match self.node(id).kind {
            NodeKind::Text { style, .. }
            | NodeKind::Svg {
                text_style: style, ..
            } => Some(self.text_styles[usize::from(style)]),
            _ => None,
        }
    }

    /// keeps room for one more resolved text style, for a node that changes the style it
    /// inherits
    fn reserve_text_style_change(&mut self) -> Result<(), MountError> {
        self.reserve_resolved_text_styles(self.text_style_changes + 1)?;
        self.text_style_changes += 1;

        Ok(())
    }

    /// keeps room for the styles of `changes` nodes that change their text style, and
    /// the default
    fn reserve_resolved_text_styles(&mut self, changes: usize) -> Result<(), MountError> {
        let needed = changes + 1;

        self.resolved_text_styles
            .reserve(needed.saturating_sub(self.resolved_text_styles.len()))
    }

    pub(crate) fn interaction_styles_for_element(
        &self,
        element: ElementStateId,
    ) -> Option<&InteractionStyles> {
        let extras = self.extras.iter().find(|extras| {
            !self.node(extras.owner).detached && extras.element_state_id == Some(element)
        })?;

        extras
            .interaction
            .styles
            .map(|index| &self.interaction_styles[usize::from(index)].1)
    }

    fn extras(&self, id: NodeId) -> Option<&NodeExtras> {
        self.node(id)
            .extras
            .map(|index| &self.extras[usize::from(index)])
    }

    pub(crate) fn extras_mut(&mut self, id: NodeId) -> Option<&mut NodeExtras> {
        self.node(id)
            .extras
            .map(|index| &mut self.extras[usize::from(index)])
    }

    /// the node's extras, adding empty ones if it has none
    fn extras_or_insert(&mut self, id: NodeId) -> Result<&mut NodeExtras, MountError> {
        let index = match self.node(id).extras {
            Some(index) => index,
            None => {
                // each belongs to a node, so they never outnumber the nodes
                let index = u16::try_from(self.extras.len()).map_err(|_| MountError::NodesFull)?;
                self.extras.push(NodeExtras {
                    owner: id,
                    element_id: None,
                    element_state_id: None,
                    interaction: NodeInteraction::default(),
                })?;
                self.node_mut(id).extras = Some(index);

                index
            }
        };

        Ok(&mut self.extras[usize::from(index)])
    }

    /// scrolls any node, giving it extras if it has none
    #[cfg(test)]
    pub(crate) fn scroll_node(&mut self, id: NodeId, offset: Offset) {
        self.extras_or_insert(id)
            .expect("test pools have room")
            .interaction
            .scroll_offset = offset;
    }

    pub(crate) fn element_id(&self, id: NodeId) -> Option<ElementId> {
        self.extras(id).and_then(|extras| extras.element_id)
    }

    pub(crate) fn element_state_id(&self, id: NodeId) -> Option<ElementStateId> {
        self.extras(id).and_then(|extras| extras.element_state_id)
    }

    /// how the node takes input. Nodes without extras take none
    pub(crate) fn interaction(&self, id: NodeId) -> NodeInteraction {
        self.extras(id)
            .map(|extras| extras.interaction)
            .unwrap_or_default()
    }

    pub(crate) fn text(&self, range: TextRange) -> &str {
        let bytes = &self.text[range.start..range.start + range.len];

        // bytes were copied from a valid str
        unsafe { core::str::from_utf8_unchecked(bytes) }
    }

    #[cfg(test)]
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

    pub(crate) fn cached_measurement(
        &self,
        node: NodeId,
        available: AvailableSize,
    ) -> Option<Size> {
        match self.node_cache[node.index()] {
            NodeCache::Measurement(cache) if cache.available == available => Some(cache.measured),
            _ => None,
        }
    }

    pub(crate) fn cache_measurement(
        &mut self,
        node: NodeId,
        available: AvailableSize,
        measured: Size,
    ) {
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
            NodeCache::Empty | NodeCache::Measurement(_) | NodeCache::MovedTo(_) => None,
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
