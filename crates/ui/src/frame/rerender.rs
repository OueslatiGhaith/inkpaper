//! partial rebuilds: entities that need to render again get a new subtree, and the rest
//! of the frame stays.
//!
//! the frame arena only appends, so a re-rendered entity's new subtree goes at the end
//! and its entity node points at it. The old subtree stays in the arena as detached
//! nodes until the next full rebuild clears the frame. Each entity's subtree was always
//! mounted apart from its parent's, so swapping subtrees at entity nodes keeps the
//! sibling order that paint relies on.

use super::{FrameArena, FrameStore, MountError, NodeId, NodeKind};

use crate::{
    EntityId, RuntimeCx, count_metric,
    element::state::{ElementStateTable, IdentityError},
    storage::{ElementStateStorage, FrameStorage},
};

impl<F: FrameStorage> FrameArena<F> {
    /// how many nodes previous partial rebuilds left behind
    pub(crate) fn detached_node_count(&self) -> usize {
        self.detached_nodes
    }

    /// the entities that rendered or left the frame in the last rebuild. Their callbacks
    /// and element states from before it are stale
    pub(crate) fn stale_entities(&self) -> &[EntityId] {
        &self.stale_entities
    }

    /// resolves the identities under the entity nodes the last partial rebuild rendered
    /// again. None is inside another, so their subtrees hold every node it mounted
    pub(crate) fn resolve_rerendered_identities<E: ElementStateStorage>(
        &mut self,
        states: &mut ElementStateTable<E>,
        frame_generation: u32,
    ) -> Result<(), IdentityError> {
        for index in 0..self.rerendered_nodes.len() {
            let node = self.rerendered_nodes[index];
            self.resolve_identities(node, states, frame_generation)?;
        }

        Ok(())
    }

    /// renders again the entities under `root` that need it, keeping the subtrees of
    /// the others. Child entities that a new render mounts keep their subtree when
    /// they don't need to render themselves.
    ///
    /// on error the frame is inconsistent and must be cleared
    pub(crate) fn rerender(
        &mut self,
        root: NodeId,
        runtime: RuntimeCx<'_>,
    ) -> Result<(), MountError> {
        self.detached_entities.clear();
        self.stale_entities.clear();
        self.rerendered_nodes.clear();

        let start = self.nodes.len();
        let mut current = Some(root);

        while let Some(node) = current {
            let NodeKind::Entity { entity, .. } = self.node(node).kind else {
                current = self.next_depth_first_node_within(node, root);
                continue;
            };
            if !runtime.entities.needs_render(entity) {
                current = self.next_depth_first_node_within(node, root);
                continue;
            }

            self.rerendered_nodes.push(node, MountError::NodesFull)?;
            self.detach_content(node)?;
            self.render_entity_node(node, runtime)?;

            // the new subtree is expanded below, with the other new nodes
            current = self.next_node_after_subtree_within(node, root);
        }

        self.expand_entities_from(start, runtime)?;

        // the detached entities that no render mounted again have left the frame
        while let Some(&(entity, node)) = self.detached_entities.last() {
            self.detached_entities
                .truncate(self.detached_entities.len() - 1);
            self.stale_entities.push(entity, MountError::NodesFull)?;
            self.detach_content(node)?;
        }

        Ok(())
    }

    /// renders the unexpanded entity nodes from `start` on, including the ones those
    /// renders mount. An entity mounted again keeps its detached subtree unless it needs
    /// to render
    pub(super) fn expand_entities_from(
        &mut self,
        start: usize,
        runtime: RuntimeCx<'_>,
    ) -> Result<(), MountError> {
        let mut index = start;

        while index < self.nodes.len() {
            let node = NodeId::new(index as u16);
            index += 1;

            let NodeKind::Entity {
                entity,
                expanded: false,
                ..
            } = self.node(node).kind
            else {
                continue;
            };

            if let Some(previous) = self.take_detached_entity(entity) {
                if !runtime.entities.needs_render(entity) {
                    self.move_content(previous, node);
                    continue;
                }

                self.detach_content(previous)?;
            }

            self.render_entity_node(node, runtime)?;
        }

        Ok(())
    }

    fn render_entity_node(
        &mut self,
        node: NodeId,
        runtime: RuntimeCx<'_>,
    ) -> Result<(), MountError> {
        let NodeKind::Entity { entity, render, .. } = self.node(node).kind else {
            unreachable!("only entity nodes render");
        };

        // a notify while rendering keeps the entity dirty for the next frame
        runtime.entities.mark_rendered(entity);
        self.stale_entities.push(entity, MountError::NodesFull)?;

        count_metric!(self, entity_render_calls);
        let content = render(entity, runtime, self)?;
        self.append_child(node, content);
        self.set_expanded(node);

        Ok(())
    }

    /// cuts the subtree under an entity node from the tree. Entity nodes inside it
    /// become detached entities, so a new render can mount their subtrees again
    fn detach_content(&mut self, entity_node: NodeId) -> Result<(), MountError> {
        let Some(content) = self.node(entity_node).first_child else {
            return Ok(());
        };

        let mut current = Some(content);
        while let Some(node) = current {
            self.node_mut(node).detached = true;
            self.detached_nodes += 1;

            let NodeKind::Entity { entity, .. } = self.node(node).kind else {
                current = self.next_depth_first_node_within(node, content);
                continue;
            };

            self.unindex_entity(entity);
            self.detached_entities
                .push((entity, node), MountError::NodesFull)?;

            current = self.next_node_after_subtree_within(node, content);
        }

        let entity_node = self.node_mut(entity_node);
        entity_node.first_child = None;
        entity_node.last_child = None;

        Ok(())
    }

    /// moves the subtree of a detached entity node under the node mounting it again
    fn move_content(&mut self, from: NodeId, to: NodeId) {
        let content = self.node(from).first_child;
        debug_assert_eq!(content, self.node(from).last_child);

        if let Some(content) = content {
            self.node_mut(content).parent = Some(to);
        }

        let from = self.node_mut(from);
        from.first_child = None;
        from.last_child = None;

        let to_node = self.node_mut(to);
        to_node.first_child = content;
        to_node.last_child = content;
        self.set_expanded(to);
    }

    fn set_expanded(&mut self, node: NodeId) {
        match &mut self.node_mut(node).kind {
            NodeKind::Entity { expanded, .. } => *expanded = true,
            _ => unreachable!("only entity nodes expand"),
        }
    }

    fn take_detached_entity(&mut self, entity: EntityId) -> Option<NodeId> {
        let position = self
            .detached_entities
            .iter()
            .position(|&(detached, _)| detached == entity)?;
        let (_, node) = self.detached_entities[position];
        let last = self.detached_entities.len() - 1;

        self.detached_entities.swap(position, last);
        self.detached_entities.truncate(last);

        Some(node)
    }

    fn unindex_entity(&mut self, entity: EntityId) {
        if let Ok(position) = self.entity_node_position(entity) {
            self.entity_nodes[position..].rotate_left(1);
            self.entity_nodes.truncate(self.entity_nodes.len() - 1);
        }
    }
}
