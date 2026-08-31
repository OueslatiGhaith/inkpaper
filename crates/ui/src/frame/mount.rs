use core::{any::TypeId, cell::Cell};

use super::{FrameArena, MountError, NodeId, NodeKind, TextRange};

use crate::{
    AppContext, CanvasDraw, CanvasStyle, Element, ElementId, EntityId, EntityRenderFn,
    EventBinding, EventBindingId, EventCallbacks, ImageSource, ImageStyle, IntoElement, Node,
    StatefulInteractivity, Style, TextStyle,
    callback::{CallbackId, CallbackStore},
    count_metric,
    entity::EntityStore,
    global::GlobalStore,
};

impl<const NODES: usize, const TEXT_BYTES: usize> FrameArena<NODES, TEXT_BYTES> {
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
