use super::{FrameArena, NodeId, NodeKind};

use crate::{
    Offset,
    element::state::{ElementStateId, ElementStateTable, IdentityError, IdentityParent},
    interaction::scroll::ScrollStateTable,
    storage::{ElementStateStorage, FrameStorage},
};

impl<F: FrameStorage> FrameArena<F> {
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
    /// resolves the identities of the subtree under `root`, which may be the frame's
    /// root or an entity node.
    ///
    /// walks in tree order rather than arena order, so each node's identity scope is
    /// resolved before the node itself wherever its subtree sits in the arena
    pub(crate) fn resolve_identities<E: ElementStateStorage>(
        &mut self,
        root: NodeId,
        states: &mut ElementStateTable<E>,
        frame_generation: u32,
    ) -> Result<(), IdentityError> {
        let mut current = Some(root);

        while let Some(node_id) = current {
            if let Some(local_id) = self.node(node_id).element_id {
                let parent = self
                    .identity_parent(node_id)
                    .ok_or(IdentityError::MissingEntityScope { node: node_id })?;

                let state_id = states.resolve(parent, local_id, frame_generation)?;

                self.node_mut(node_id).element_state_id = Some(state_id);
            }

            current = self.next_depth_first_node_within(node_id, root);
        }

        Ok(())
    }

    /// sets the elements whose focused and pressed styles [`Self::style`] applies
    pub(crate) fn set_interaction_state(
        &mut self,
        focused: Option<ElementStateId>,
        pressed: Option<ElementStateId>,
    ) {
        self.focused = focused;
        self.pressed = pressed;
    }
    pub(crate) fn resolve_scroll_offsets<E: ElementStateStorage>(
        &mut self,
        states: &ScrollStateTable<E>,
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
                    .style(node_id)
                    .expect("div node must have style")
                    .text
                    .resolve(inherited),
                NodeKind::Text { .. } | NodeKind::Svg { .. } => {
                    self.node(node_id).text_style.resolve(inherited)
                }
                NodeKind::Image { .. } | NodeKind::Entity { .. } | NodeKind::Canvas { .. } => {
                    inherited
                }
            };

            self.node_mut(node_id).effective_text_style = resolved;
            current = self.next_depth_first_node(node_id);
        }
    }
}
