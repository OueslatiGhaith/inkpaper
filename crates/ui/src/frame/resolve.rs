use super::{FrameArena, NodeId, NodeKind};

use crate::{
    Offset, ResolvedTextStyle,
    element::state::{ElementStateId, ElementStateTable, IdentityError, IdentityParent},
    interaction::scroll::ScrollStateTable,
};

impl FrameArena {
    fn identity_parent(&self, node: NodeId) -> Option<IdentityParent> {
        let mut current = self.nodes[node.index()].parent;

        while let Some(parent_id) = current {
            let parent = &self.nodes[parent_id.index()];

            // entity in an identity boundary
            if let NodeKind::Entity { entity, .. } = parent.kind {
                return Some(IdentityParent::Entity(entity));
            }
            // otherwise the nearest identified ancestor establishes the scope
            if let Some(state_id) = self.element_state_id(parent_id) {
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
    pub(crate) fn resolve_identities(
        &mut self,
        root: NodeId,
        states: &mut ElementStateTable,
        frame_generation: u32,
    ) -> Result<(), IdentityError> {
        let mut current = Some(root);

        while let Some(node_id) = current {
            if let Some(local_id) = self.element_id(node_id) {
                let parent = self
                    .identity_parent(node_id)
                    .ok_or(IdentityError::MissingEntityScope { node: node_id })?;

                let state_id = states.resolve(parent, local_id, frame_generation)?;

                self.extras_mut(node_id)
                    .expect("identified nodes have extras")
                    .element_state_id = Some(state_id);
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
    /// nodes without extras don't scroll
    pub(crate) fn resolve_scroll_offsets(&mut self, states: &ScrollStateTable) {
        for index in 0..self.extras.len() {
            let extras = &mut self.extras[index];
            if self.nodes[extras.owner.index()].detached {
                continue;
            }

            extras.interaction.scroll_offset = extras
                .element_state_id
                .map(|id| states.offset(id))
                .unwrap_or(Offset::ZERO);
        }
    }
    /// finds each node's text style from the styles it inherits, keeping each distinct
    /// style once. Parents come before their children, so each inherits from this pass
    pub(crate) fn resolve_text_styles(&mut self, root: NodeId) {
        self.resolved_text_styles.clear();

        let mut current = Some(root);
        while let Some(node_id) = current {
            let inherited = self
                .node(node_id)
                .parent
                .map(|parent| self.resolved_text_style(parent))
                .unwrap_or_default();

            let resolved = match self.node(node_id).kind {
                NodeKind::Div { .. } => self
                    .style(node_id)
                    .expect("div node must have style")
                    .text
                    .resolve(inherited),
                NodeKind::Text { style, .. }
                | NodeKind::Svg {
                    text_style: style, ..
                } => self.text_styles[usize::from(style)].resolve(inherited),
                NodeKind::Image { .. } | NodeKind::Entity { .. } | NodeKind::Canvas { .. } => {
                    inherited
                }
            };

            self.node_mut(node_id).resolved_text_style = self.intern_text_style(resolved);
            current = self.next_depth_first_node(node_id);
        }
    }

    /// the index of `style` in the resolved text styles, adding it if it's new
    fn intern_text_style(&mut self, style: ResolvedTextStyle) -> u16 {
        let index = match self
            .resolved_text_styles
            .iter()
            .position(|resolved| *resolved == style)
        {
            Some(index) => index,
            None => {
                self.resolved_text_styles
                    .push(style)
                    .expect("mounting keeps room for every style a pass can find");
                self.resolved_text_styles.len() - 1
            }
        };

        u16::try_from(index).expect("there are fewer resolved text styles than nodes")
    }
}
