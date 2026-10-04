use core::any::TypeId;

use super::{FrameArena, InteractionStyles, MountError, NodeId, NodeKind, TextRange};

use crate::{
    AppContext, CanvasDraw, CanvasStyle, Element, ElementId, EntityId, EntityRenderFn,
    EventBinding, EventBindingId, EventCallbacks, ImageSource, ImageStyle, IntoElement, Node,
    RuntimeCx, StatefulInteractivity, Style, SvgSource, SvgStyle, TextStyle, callback::CallbackId,
    count_metric, frame::NodeCache, storage::FrameStorage,
};

impl<F: FrameStorage> FrameArena<F> {
    fn push_node(&mut self, kind: NodeKind) -> Result<NodeId, MountError> {
        let index = u16::try_from(self.nodes.len()).map_err(|_| MountError::NodesFull)?;
        self.nodes.reserve(1, MountError::NodesFull)?;
        self.node_cache.reserve(1, MountError::NodesFull)?;

        let id = NodeId::new(index);
        self.nodes.push(Node::new(kind), MountError::NodesFull)?;
        self.node_cache
            .push(NodeCache::Empty, MountError::NodesFull)?;

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
    pub(crate) fn expand_entities(&mut self, runtime: RuntimeCx<'_>) -> Result<(), MountError> {
        self.stale_entities.clear();
        self.expand_entities_from(0, runtime)
    }
    pub(crate) fn mount_and_expand<E>(
        &mut self,
        element: E,
        runtime: RuntimeCx<'_>,
    ) -> Result<NodeId, MountError>
    where
        E: IntoElement,
    {
        let app = AppContext::from_globals(runtime.globals);
        let root = self.mount(element, app)?;
        self.expand_entities(runtime)?;

        Ok(root)
    }
    pub(crate) fn bind_event(
        &mut self,
        node: NodeId,
        event_type: TypeId,
        callback: CallbackId,
    ) -> Result<(), MountError> {
        let index =
            u16::try_from(self.event_bindings.len()).map_err(|_| MountError::EventBindingsFull)?;
        let binding_id = EventBindingId::new(index);

        self.event_bindings.push(
            EventBinding {
                event_type,
                callback,
                next: None,
            },
            MountError::EventBindingsFull,
        )?;

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
            &self.event_bindings,
            self.node(node).first_event_binding,
            event_type,
        )
    }
}

pub(crate) trait FrameStore {
    fn push_div(&mut self, style: Style) -> Result<NodeId, MountError>;

    fn push_text(&mut self, text: &str, style: TextStyle) -> Result<NodeId, MountError>;

    fn push_image(&mut self, source: ImageSource, style: ImageStyle) -> Result<NodeId, MountError>;

    fn push_svg(
        &mut self,
        source: SvgSource,
        style: SvgStyle,
        text_style: TextStyle,
    ) -> Result<NodeId, MountError>;

    fn push_canvas(&mut self, draw: CanvasDraw, style: CanvasStyle) -> Result<NodeId, MountError>;

    fn push_entity(
        &mut self,
        entity: EntityId,
        render: EntityRenderFn,
    ) -> Result<NodeId, MountError>;

    fn append_child(&mut self, parent: NodeId, child: NodeId);

    fn identify(&mut self, node: NodeId, id: ElementId);

    fn apply_interactivity(
        &mut self,
        node: NodeId,
        interactivity: StatefulInteractivity,
    ) -> Result<(), MountError>;

    fn bind_event(
        &mut self,
        node: NodeId,
        event_type: TypeId,
        callback: CallbackId,
    ) -> Result<(), MountError>;
}

impl<F: FrameStorage> FrameStore for FrameArena<F> {
    fn push_div(&mut self, style: Style) -> Result<NodeId, MountError> {
        self.push_node(NodeKind::Div { style })
    }

    fn push_text(&mut self, text: &str, style: TextStyle) -> Result<NodeId, MountError> {
        let start = self.text.len();

        self.text
            .extend_from_slice(text.as_bytes(), MountError::TextStorageFull)?;

        let range = TextRange {
            start,
            len: text.len(),
        };

        let node = match self.push_node(NodeKind::Text { text: range }) {
            Ok(node) => node,
            Err(error) => {
                self.text.truncate(start);
                return Err(error);
            }
        };

        self.node_mut(node).text_style = style;

        Ok(node)
    }

    fn push_image(&mut self, source: ImageSource, style: ImageStyle) -> Result<NodeId, MountError> {
        self.push_node(NodeKind::Image { source, style })
    }

    fn push_svg(
        &mut self,
        source: SvgSource,
        style: SvgStyle,
        text_style: TextStyle,
    ) -> Result<NodeId, MountError> {
        let node = self.push_node(NodeKind::Svg { source, style })?;

        self.node_mut(node).text_style = text_style;

        Ok(node)
    }

    fn push_canvas(&mut self, draw: CanvasDraw, style: CanvasStyle) -> Result<NodeId, MountError> {
        self.push_node(NodeKind::Canvas { draw, style })
    }

    fn push_entity(
        &mut self,
        entity: EntityId,
        render: EntityRenderFn,
    ) -> Result<NodeId, MountError> {
        let Err(position) = self.entity_node_position(entity) else {
            return Err(MountError::DuplicateEntityMount(entity));
        };

        // reserve the index entry first, so a mounted entity node is always indexed
        self.entity_nodes.reserve(1, MountError::NodesFull)?;
        let node = self.push_node(NodeKind::Entity {
            entity,
            render,
            expanded: false,
        })?;

        self.entity_nodes
            .push((entity, node), MountError::NodesFull)?;
        self.entity_nodes[position..].rotate_right(1);

        Ok(node)
    }

    fn append_child(&mut self, parent: NodeId, child: NodeId) {
        let parent_idx = parent.index();
        let child_idx = child.index();
        // a child may be mounted before its parent, so an existing subtree can be
        // attached under a newly mounted node
        debug_assert!(child != parent, "a frame node cannot be its own child");
        debug_assert!(
            self.nodes[child_idx].parent.is_none(),
            "a frame node can only be attached once"
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

    fn apply_interactivity(
        &mut self,
        node: NodeId,
        interactivity: StatefulInteractivity,
    ) -> Result<(), MountError> {
        let styles = InteractionStyles {
            focused: interactivity.focused_style,
            pressed: interactivity.pressed_style,
        };

        if !styles.is_empty() {
            match self.nodes[node.index()].interaction.styles {
                Some(index) => {
                    let existing = &mut self.interaction_styles[usize::from(index)];
                    *existing = existing.merge(styles);
                }
                None => {
                    let index = u16::try_from(self.interaction_styles.len())
                        .map_err(|_| MountError::InteractionStylesFull)?;
                    self.interaction_styles
                        .push(styles, MountError::InteractionStylesFull)?;
                    self.nodes[node.index()].interaction.styles = Some(index);
                }
            }
        }

        let interaction = &mut self.nodes[node.index()].interaction;
        interaction.focusable |= interactivity.focusable;
        if interactivity.scroll_axes.any() {
            interaction.scroll_axes = interactivity.scroll_axes;
        }
        if interactivity.initial_scroll_child.is_some() {
            interaction.initial_scroll_child = interactivity.initial_scroll_child;
        }

        Ok(())
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

    pub(crate) fn push_svg(
        &mut self,
        source: SvgSource,
        style: SvgStyle,
        text_style: TextStyle,
    ) -> Result<NodeId, MountError> {
        self.frame.push_svg(source, style, text_style)
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
    ) -> Result<(), MountError> {
        self.frame.apply_interactivity(node, interactivity)
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
