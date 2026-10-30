//! reclaims what partial rebuilds leave behind. A re-rendered entity's old subtree
//! stays in the arena as detached nodes, along with its text, event bindings and
//! interaction styles, until the frame compacts.

use super::{FrameArena, NodeCache, NodeId, NodeKind};

use crate::{EventBinding, storage::FrameStorage};

impl<F: FrameStorage> FrameArena<F> {
    /// removes the detached nodes, with the text, event bindings and interaction styles
    /// only they used. The other nodes keep their order, so the ones a partial rebuild
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
        self.compact_event_bindings();
        self.compact_interaction_styles();
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
            if let NodeKind::Text { text } = &mut node.kind {
                debug_assert!(used <= text.start, "text is in node order");

                self.text
                    .copy_within(text.start..text.start + text.len, used);
                text.start = used;
                used += text.len;
            }
        }

        self.text.truncate(used);
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

            self.node_mut(owner).interaction.styles =
                Some(u16::try_from(kept).expect("interaction style indices fit in u16"));
            self.interaction_styles[kept] = (self.moved(owner), styles);
            kept += 1;
        }

        self.interaction_styles.truncate(kept);
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
