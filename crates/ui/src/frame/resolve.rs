use super::{FrameArena, NodeId, NodeKind};

use crate::{
    Offset,
    element_state::{ElementStateId, ElementStateTable, IdentityError, IdentityParent},
    scroll::ScrollStateTable,
};

impl<const NODES: usize, const TEXT_BYTES: usize> FrameArena<NODES, TEXT_BYTES> {
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
}
