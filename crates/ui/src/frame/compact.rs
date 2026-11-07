//! reclaims what partial rebuilds leave behind. A re-rendered entity's old subtree
//! stays in the arena as detached nodes, along with its text, side table entries,
//! event bindings, interaction styles and extras, until the frame compacts.

use super::{FrameArena, Node, NodeCache, NodeExtras, NodeId, NodeKind, storage::FrameBuffer};

use crate::{EventBinding, TextStyle};

impl FrameArena {
    /// removes the detached nodes, with the text, side table entries, event bindings,
    /// interaction styles and extras only they used. The other nodes keep their order, so the ones a partial rebuild
    /// mounted still come after the rest, but they get new ids. Returns `root`'s.
    ///
    /// drops the layout caches, which the layout after a rebuild fills again
    pub(crate) fn compact(&mut self, root: NodeId) -> NodeId {
        if self.detached_nodes == 0 {
            return root;
        }
        debug_assert!(
            self.detached_entities.is_empty(),
            "the frame compacts between rebuilds"
        );

        self.plan_moves();
        self.compact_text();
        self.compact_side_tables();
        self.compact_event_bindings();
        self.compact_interaction_styles();
        self.compact_extras();
        self.count_text_style_changes();
        self.relink();
        let root = self.moved(root);
        self.move_nodes();

        root
    }

    /// records where each live node moves in its cache slot
    fn plan_moves(&mut self) {
        let mut new_ids = self.node_ids();
        let mut live = 0;
        let mut layout_start = None;

        for index in 0..self.nodes.len() {
            if self.tracked_layout_start == Some(index) {
                layout_start = Some(live);
            }

            self.node_cache[index] = if self.nodes[index].detached {
                NodeCache::Empty
            } else {
                live += 1;
                NodeCache::MovedTo(new_ids.next().expect("every node has an id"))
            };
        }

        // the first node the last partial rebuild mounted is the first live one after
        // the nodes before it
        self.tracked_layout_start = self
            .tracked_layout_start
            .map(|_| layout_start.unwrap_or(live));
    }

    fn moved(&self, node: NodeId) -> NodeId {
        match self.node_cache[node.index()] {
            NodeCache::MovedTo(moved) => moved,
            _ => unreachable!("live nodes only link to live nodes"),
        }
    }

    /// text is appended as its node mounts, so live text keeps node order
    fn compact_text(&mut self) {
        let mut used = 0;

        for node in self.nodes.iter_mut().filter(|node| !node.detached) {
            if let NodeKind::Text { text, .. } = &mut node.kind {
                let bytes = text.bytes();
                debug_assert!(used <= bytes.start, "text is in node order");

                self.text.copy_within(bytes, used);
                text.start = u32::try_from(used).expect("compacted text starts earlier");
                used += text.len as usize;
            }
        }

        self.text.truncate(used);
    }

    /// moves the side tables' live entries down. Their entries are appended as their
    /// nodes mount, so live entries keep node order
    fn compact_side_tables(&mut self) {
        compact_in_node_order(&mut self.nodes, &mut self.styles, |kind| match kind {
            NodeKind::Div { style } => Some(style),
            _ => None,
        });
        compact_in_node_order(&mut self.nodes, &mut self.text_styles, |kind| match kind {
            NodeKind::Text { style, .. }
            | NodeKind::Svg {
                text_style: style, ..
            } => Some(style),
            _ => None,
        });
        compact_in_node_order(&mut self.nodes, &mut self.images, |kind| match kind {
            NodeKind::Image { image } => Some(image),
            _ => None,
        });
        compact_in_node_order(&mut self.nodes, &mut self.svgs, |kind| match kind {
            NodeKind::Svg { svg, .. } => Some(svg),
            _ => None,
        });
        compact_in_node_order(&mut self.nodes, &mut self.canvases, |kind| match kind {
            NodeKind::Canvas { canvas } => Some(canvas),
            _ => None,
        });
    }

    /// counts the live nodes that can change their text style, once the tables hold
    /// only theirs
    fn count_text_style_changes(&mut self) {
        let changes = |style: &TextStyle| *style != TextStyle::DEFAULT;

        self.text_style_changes = self
            .styles
            .iter()
            .filter(|style| changes(&style.text))
            .count()
            + self
                .text_styles
                .iter()
                .filter(|style| changes(style))
                .count()
            + self.interaction_styles.len();
    }

    fn compact_event_bindings(&mut self) {
        let mut kept = 0;
        let mut previous_owner = None;

        for index in 0..self.event_bindings.len() {
            let binding = self.event_bindings[index];
            let owner = binding.node;
            if self.node(owner).detached {
                continue;
            }

            // a node's bindings sit together, so a new owner starts its run
            if previous_owner != Some(owner) {
                self.node_mut(owner).event_bindings.start =
                    u16::try_from(kept).expect("binding indices fit in u16");
                previous_owner = Some(owner);
            }

            self.event_bindings[kept] = EventBinding {
                node: self.moved(owner),
                ..binding
            };
            kept += 1;
        }

        self.event_bindings.truncate(kept);
    }

    fn compact_interaction_styles(&mut self) {
        let mut kept = 0;

        for index in 0..self.interaction_styles.len() {
            let (owner, styles) = self.interaction_styles[index];
            if self.node(owner).detached {
                continue;
            }

            self.extras_mut(owner)
                .expect("nodes with interaction styles have extras")
                .interaction
                .styles = Some(u16::try_from(kept).expect("interaction style indices fit in u16"));
            self.interaction_styles[kept] = (self.moved(owner), styles);
            kept += 1;
        }

        self.interaction_styles.truncate(kept);
    }

    /// after the interaction styles, which find their nodes' extras by the old indices
    fn compact_extras(&mut self) {
        let mut kept = 0;

        for index in 0..self.extras.len() {
            let extras = self.extras[index];
            if self.node(extras.owner).detached {
                continue;
            }

            self.node_mut(extras.owner).extras =
                Some(u16::try_from(kept).expect("extras indices fit in u16"));
            self.extras[kept] = NodeExtras {
                owner: self.moved(extras.owner),
                ..extras
            };
            kept += 1;
        }

        self.extras.truncate(kept);
    }

    /// points the links of live nodes, and the frame's own node lists, at the nodes'
    /// new ids
    fn relink(&mut self) {
        for index in 0..self.nodes.len() {
            let node = self.nodes[index];
            if node.detached {
                continue;
            }

            let moved = |link: Option<NodeId>| link.map(|link| self.moved(link));
            let (parent, first_child, last_child, next_sibling) = (
                moved(node.parent),
                moved(node.first_child),
                moved(node.last_child),
                moved(node.next_sibling),
            );

            let node = &mut self.nodes[index];
            node.parent = parent;
            node.first_child = first_child;
            node.last_child = last_child;
            node.next_sibling = next_sibling;
        }

        for index in 0..self.entity_nodes.len() {
            let (entity, node) = self.entity_nodes[index];
            self.entity_nodes[index] = (entity, self.moved(node));
        }
        for index in 0..self.rerendered_nodes.len() {
            self.rerendered_nodes[index] = self.moved(self.rerendered_nodes[index]);
        }
    }

    fn move_nodes(&mut self) {
        let mut live = 0;

        for index in 0..self.nodes.len() {
            if !self.nodes[index].detached {
                self.nodes[live] = self.nodes[index];
                live += 1;
            }
        }

        self.nodes.truncate(live);
        self.node_cache.truncate(live);
        self.node_cache.fill(NodeCache::Empty);
        self.subtree_paint_bounds_valid = false;
        self.detached_nodes = 0;
    }
}

/// moves the live entries of `table` down, keeping their order, and points each live
/// node at its entry's new index. `index` gives a node's index into `table`, if it has
/// one
fn compact_in_node_order<T: Copy>(
    nodes: &mut [Node],
    table: &mut FrameBuffer<T>,
    index: impl Fn(&mut NodeKind) -> Option<&mut u16>,
) {
    let mut used = 0;

    for node in nodes.iter_mut().filter(|node| !node.detached) {
        if let Some(index) = index(&mut node.kind) {
            debug_assert!(used <= usize::from(*index), "entries are in node order");

            table[used] = table[usize::from(*index)];
            *index = u16::try_from(used).expect("side table indices fit in u16");
            used += 1;
        }
    }

    table.truncate(used);
}
