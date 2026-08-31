use heapless::Vec;

use crate::{
    CanvasDraw, CanvasStyle, ElementId, EntityAccessError, EntityId, EntityRenderFn, EventBinding,
    EventBindingId, ImageSource, ImageStyle, Offset, Rect, ResolvedTextStyle, Size,
    StatefulInteractivity, Style, StylePatch, TextStyle, element::state::ElementStateId,
    interaction::scroll::ScrollAxes,
};
#[cfg(feature = "metrics")]
use crate::{PerformanceMetrics, PerformanceMetricsCell};

mod mount;
mod resolve;
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
