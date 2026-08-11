use core::cell::Cell;

use heapless::Vec;

use crate::{
    Element, ElementId, EntityAccessError, EntityId, EntityRenderFn, IntoElement, ListenerId,
    Offset, Rect, StatefulInteractivity, Style, StylePatch, TextStyle,
    element_state::{ElementStateId, ElementStateTable, IdentityError, IdentityParent},
    entity_store::EntityStore,
    listener_store::ListenerStore,
    scroll::{ScrollAxes, ScrollStateTable},
    text_style::TextStylePatch,
};

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
    pub(crate) text_style: TextStylePatch,
    pub(crate) effective_text_style: TextStyle,
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
            text_style: TextStylePatch::default(),
            effective_text_style: TextStyle::default(),
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
    pub(crate) click: Option<ListenerId>,
    pub(crate) focused_style: StylePatch,
    pub(crate) pressed_style: StylePatch,
    pub(crate) scroll_axes: ScrollAxes,
    pub(crate) scroll_offset: Offset,
}

impl From<StatefulInteractivity> for NodeInteraction {
    fn from(value: StatefulInteractivity) -> Self {
        Self {
            click: value.click,
            focused_style: value.focused_style,
            pressed_style: value.pressed_style,
            scroll_axes: value.scroll_axes,
            scroll_offset: Offset::ZERO,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MountError {
    NodesFull,
    TextStorageFull,
    EntityAccess(EntityAccessError),
    DuplicateEntityMount(EntityId),
}

impl From<EntityAccessError> for MountError {
    fn from(value: EntityAccessError) -> Self {
        Self::EntityAccess(value)
    }
}

pub(crate) struct FrameArena<const NODES: usize, const TEXT_BYTES: usize> {
    pub(crate) nodes: Vec<Node, NODES>,
    text: Vec<u8, TEXT_BYTES>,
}

impl<const NODES: usize, const TEXT_BYTES: usize> Default for FrameArena<NODES, TEXT_BYTES> {
    fn default() -> Self {
        Self {
            nodes: Vec::new(),
            text: Vec::new(),
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
    }

    fn push_node(&mut self, kind: NodeKind) -> Result<NodeId, MountError> {
        if self.nodes.len() >= NODES || self.nodes.len() > u16::MAX as usize {
            return Err(MountError::NodesFull);
        }

        let id = NodeId::new(self.nodes.len() as u16);
        self.nodes
            .push(Node::new(kind))
            .map_err(|_| MountError::NodesFull)?;

        Ok(id)
    }

    pub fn mount<E>(&mut self, element: E) -> Result<NodeId, MountError>
    where
        E: IntoElement,
    {
        let element = element.into_element();
        let mut cx = MountCx::new(self);

        element.mount(&mut cx)
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
        listeners: &dyn ListenerStore,
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
                let entity_node = NodeId::new(index as u16);
                let root = render(entity, entities, listeners, notified, self)?;
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
        listeners: &dyn ListenerStore,
        notified: &Cell<bool>,
    ) -> Result<NodeId, MountError>
    where
        E: IntoElement,
    {
        let root = self.mount(element)?;
        self.expand_entities(entities, listeners, notified)?;

        Ok(root)
    }

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
                NodeKind::Div { .. } => {
                    let style = self
                        .node(node_id)
                        .style()
                        .expect("div node must have style");

                    TextStylePatch::fron_style(style).resolve(inherited)
                }
                NodeKind::Text { .. } => self.node(node_id).text_style.resolve(inherited),
                NodeKind::Entity { .. } => inherited,
            };

            self.node_mut(node_id).effective_text_style = resolved;
            current = self.next_depth_first_node(node_id);
        }
    }
}

impl<const NODES: usize, const TEXT_BYTES: usize> FrameStore for FrameArena<NODES, TEXT_BYTES> {
    fn push_div(&mut self, style: Style) -> Result<NodeId, MountError> {
        self.push_node(NodeKind::Div { style })
    }

    fn push_text(&mut self, text: &str, style: TextStylePatch) -> Result<NodeId, MountError> {
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

        self.nodes[child_idx].parent = Some(parent);

        match self.nodes[parent_idx].last_child {
            Some(last) => self.nodes[last.index()].next_sibling = Some(child),
            None => self.nodes[parent_idx].first_child = Some(child),
        }

        self.nodes[parent_idx].last_child = Some(child);
    }

    fn make_stateful(&mut self, node: NodeId, id: ElementId, interaction: NodeInteraction) {
        let node = &mut self.nodes[node.index()];
        node.element_id = Some(id);
        node.interaction = interaction;
    }
}

pub(crate) trait FrameStore {
    fn push_div(&mut self, style: Style) -> Result<NodeId, MountError>;
    fn push_text(&mut self, text: &str, style: TextStylePatch) -> Result<NodeId, MountError>;
    fn push_entity(
        &mut self,
        entity: EntityId,
        render: EntityRenderFn,
    ) -> Result<NodeId, MountError>;
    fn append_child(&mut self, parent: NodeId, child: NodeId);
    fn make_stateful(&mut self, node: NodeId, id: ElementId, interaction: NodeInteraction);
}

pub struct MountCx<'a> {
    frame: &'a mut dyn FrameStore,
}

impl<'a> MountCx<'a> {
    pub(crate) fn new(frame: &'a mut dyn FrameStore) -> Self {
        Self { frame }
    }
}

impl MountCx<'_> {
    pub(crate) fn push_div(&mut self, style: Style) -> Result<NodeId, MountError> {
        self.frame.push_div(style)
    }

    pub(crate) fn push_text(
        &mut self,
        text: &str,
        style: TextStylePatch,
    ) -> Result<NodeId, MountError> {
        self.frame.push_text(text, style)
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

    pub(crate) fn make_stateful(
        &mut self,
        node: NodeId,
        id: ElementId,
        interaction: NodeInteraction,
    ) {
        self.frame.make_stateful(node, id, interaction);
    }
}

#[cfg(test)]
mod tests {
    use core::cell::Cell;
    use std::string::String;

    use crate::{
        element_state::{ElementStateId, ElementStateTable, IdentityError, IdentityParent},
        listener_store::ListenerArena,
        *,
    };

    fn node_text<const N: usize, const T: usize>(frame: &FrameArena<N, T>, id: NodeId) -> &str {
        match frame.node(id).kind {
            NodeKind::Text { text, .. } => frame.text(text),
            _ => panic!("expected text node"),
        }
    }

    fn find_element<const NODES: usize, const TEXT: usize>(
        frame: &FrameArena<NODES, TEXT>,
        id: ElementId,
    ) -> Option<NodeId> {
        frame.nodes.iter().enumerate().find_map(|(index, node)| {
            (node.element_id == Some(id)).then(|| NodeId::new(index as u16))
        })
    }

    fn find_all_elements<const NODES: usize, const TEXT_BYTES: usize>(
        frame: &FrameArena<NODES, TEXT_BYTES>,
        id: ElementId,
    ) -> std::vec::Vec<NodeId> {
        frame
            .nodes
            .iter()
            .enumerate()
            .filter(|&(_, node)| node.element_id == Some(id))
            .map(|(index, _)| NodeId::new(index as u16))
            .collect()
    }

    fn state_id<const NODES: usize, const TEXT_BYTES: usize>(
        frame: &FrameArena<NODES, TEXT_BYTES>,
        node: NodeId,
    ) -> ElementStateId {
        frame
            .node(node)
            .element_state_id
            .expect("element should have resolved state identity")
    }

    fn build_frame<
        Root,
        const NODES: usize,
        const TEXT_BYTES: usize,
        const STATES: usize,
        const ENTITY_BYTES: usize,
        const ENTITY_SLOTS: usize,
        const LISTENER_BYTES: usize,
        const LISTENER_SLOTS: usize,
    >(
        frame: &mut FrameArena<NODES, TEXT_BYTES>,
        states: &mut ElementStateTable<STATES>,
        root: Entity<Root>,
        entities: &EntityArena<ENTITY_BYTES, ENTITY_SLOTS>,
        listeners: &ListenerArena<LISTENER_BYTES, LISTENER_SLOTS>,
        notified: &Cell<bool>,
        generation: u32,
    ) -> Result<NodeId, IdentityError>
    where
        Root: Render,
    {
        frame.clear();

        let root = frame.mount(root).expect("mount should succeed");

        frame
            .expand_entities(entities, listeners, notified)
            .expect("entity expansion should succeed");

        frame.resolve_identities(states, generation)?;
        states.sweep(generation);

        Ok(root)
    }

    #[test]
    fn mounts_div_tree() {
        let mut frame = FrameArena::<16, 128>::default();
        let root = frame.mount(div().child("A").child("B")).unwrap();

        assert_eq!(frame.node_count(), 3);

        let root_node = frame.node(root);
        let a = root_node.first_child.unwrap();
        let b = frame.node(a).next_sibling.unwrap();

        assert_eq!(frame.node(a).parent, Some(root));
        assert_eq!(frame.node(b).parent, Some(root));
        assert_eq!(frame.node(b).next_sibling, None);
        assert_eq!(node_text(&frame, a), "A");
        assert_eq!(node_text(&frame, b), "B");
    }

    #[test]
    fn preserves_child_order() {
        let mut frame = FrameArena::<16, 128>::default();
        let root = frame.mount(div().child("A").child("B").child("C")).unwrap();
        let a = frame.node(root).first_child.unwrap();
        let b = frame.node(a).next_sibling.unwrap();
        let c = frame.node(b).next_sibling.unwrap();

        assert_eq!(node_text(&frame, a), "A");
        assert_eq!(node_text(&frame, b), "B");
        assert_eq!(node_text(&frame, c), "C");
        assert_eq!(frame.node(c).next_sibling, None);
        assert_eq!(frame.node(root).last_child, Some(c));
    }

    #[test]
    fn mounts_nested_elements() {
        let mut frame = FrameArena::<16, 128>::default();
        let root = frame
            .mount(div().child("Top").child(div().child("Nested")))
            .unwrap();

        assert_eq!(frame.node_count(), 4);

        // Expected:
        //
        // 0 Div
        // ├── 1 Text "Top"
        // └── 2 Div
        //     └── 3 Text "Nested"

        let top = frame.node(root).first_child.unwrap();
        let nested_div = frame.node(top).next_sibling.unwrap();
        let nested_text = frame.node(nested_div).first_child.unwrap();

        assert_eq!(node_text(&frame, top), "Top");
        assert_eq!(node_text(&frame, nested_text), "Nested");
        assert_eq!(frame.node(top).parent, Some(root));
        assert_eq!(frame.node(nested_div).parent, Some(root));
        assert_eq!(frame.node(nested_text).parent, Some(nested_div));
        assert_eq!(frame.node(root).last_child, Some(nested_div));
        assert_eq!(frame.node(nested_div).last_child, Some(nested_text));
    }

    #[test]
    fn preserves_styles_when_mounting() {
        let mut frame = FrameArena::<8, 64>::default();

        let root = frame
            .mount(div().flex().flex_col().w_full().p(px(8)))
            .unwrap();

        match frame.node(root).kind {
            NodeKind::Div { style } => {
                assert_eq!(style.display, Display::Flex);
                assert_eq!(style.flex_direction, FlexDirection::Column);
                assert_eq!(style.width, Length::Fill);
                assert_eq!(style.padding.top, px(8));
                assert_eq!(style.padding.right, px(8));
                assert_eq!(style.padding.bottom, px(8));
                assert_eq!(style.padding.left, px(8));
            }
            _ => panic!("expected root to be a div node"),
        }
    }

    #[test]
    fn text_is_copied_into_frame_storage() {
        let mut frame = FrameArena::<8, 128>::default();
        let root;

        {
            let text = String::from("Hello");
            root = frame.mount(div().child(text.as_str())).unwrap();
            // `text` is dropped at the end of this block.
        }

        let text_node = frame.node(root).first_child.unwrap();

        assert_eq!(node_text(&frame, text_node), "Hello");
        assert_eq!(frame.text_bytes_used(), 5);
    }

    #[test]
    fn mounts_stateful_element_identity() {
        let mut frame = FrameArena::<8, 128>::default();
        let button = frame.mount(div().id("button").child("Press")).unwrap();

        assert_eq!(
            frame.node(button).element_id,
            Some(ElementId::Name("button"))
        );

        let text = frame.node(button).first_child.unwrap();

        assert_eq!(node_text(&frame, text), "Press");
    }

    #[test]
    fn reports_node_capacity_exhaustion() {
        let mut frame = FrameArena::<2, 128>::default();

        // required nodes:
        // 0 Div
        // 1 Text "A"
        // 2 Text "B"
        // capacity is only 2.

        let result = frame.mount(div().child("A").child("B"));

        assert_eq!(result, Err(MountError::NodesFull));
    }

    #[test]
    fn reports_text_storage_exhaustion() {
        let mut frame = FrameArena::<8, 4>::default();

        // "Hello" needs 5 UTF-8 bytes, but the frame only has 4.
        let result = frame.mount(div().child("Hello"));

        assert_eq!(result, Err(MountError::TextStorageFull));
    }

    #[test]
    fn clear_resets_frame_storage() {
        let mut frame = FrameArena::<8, 128>::default();
        frame.mount(div().child("Hello")).unwrap();

        assert_eq!(frame.node_count(), 2);
        assert_eq!(frame.text_bytes_used(), 5);

        frame.clear();

        assert_eq!(frame.node_count(), 0);
        assert_eq!(frame.text_bytes_used(), 0);

        // verify it is usable again after clearing.
        let root = frame.mount(div().child("Again")).unwrap();

        assert_eq!(root, NodeId::new(0));
        assert_eq!(frame.node_count(), 2);
        assert_eq!(frame.text_bytes_used(), 5);
    }

    struct Child;

    impl Render for Child {
        fn render<'a>(&'a mut self, _cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
            div().child("Child")
        }
    }

    struct Parent {
        child: Entity<Child>,
    }

    impl Render for Parent {
        fn render<'a>(&'a mut self, _cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
            div().child("Parent").child(self.child)
        }
    }

    #[test]
    fn mounts_entities_as_placeholders() {
        let mut frame = FrameArena::<8, 128>::default();

        // we don't need a real EntityArena for this test yet because mounting an
        // Entity<T> only stores its EntityId.
        let child = Entity::<Child>::from_id(EntityId::new(7, 0));
        let root = frame.mount(div().child(child)).unwrap();

        assert_eq!(frame.node_count(), 2);

        let child_node = frame.node(root).first_child.unwrap();

        assert_eq!(frame.node(child_node).parent, Some(root));

        match frame.node(child_node).kind {
            NodeKind::Entity { entity, .. } => assert_eq!(entity, child.entity_id()),
            _ => panic!("expected entity placeholder node"),
        }
    }

    #[test]
    fn expands_nested_entities() {
        let entities = EntityArena::<2048, 16>::default();
        let mut frame = FrameArena::<32, 256>::default();
        let listeners = ListenerArena::<2048, 16>::default();
        let notified = Cell::new(false);

        let child = entities.insert(Child).unwrap();
        let parent = entities.insert(Parent { child }).unwrap();
        let root = frame.mount(parent).unwrap();

        frame
            .expand_entities(&entities, &listeners, &notified)
            .unwrap();

        assert_eq!(frame.node_count(), 6);

        // 0: Entity<Parent>
        match frame.node(root).kind {
            NodeKind::Entity {
                entity, expanded, ..
            } => {
                assert_eq!(entity, parent.entity_id());
                assert!(expanded);
            }
            _ => panic!("expected parent entity"),
        }

        // 1: parent's rendered Div
        let parent_div = frame.node(root).first_child.unwrap();
        // 2: "parent"
        let parent_text = frame.node(parent_div).first_child.unwrap();

        assert_eq!(node_text(&frame, parent_text), "Parent");

        // 3: Entity<Child>
        let child_entity = frame.node(parent_text).next_sibling.unwrap();

        match frame.node(child_entity).kind {
            NodeKind::Entity {
                entity, expanded, ..
            } => {
                assert_eq!(entity, child.entity_id());
                assert!(expanded);
            }
            _ => panic!("expected child entity"),
        }

        // 4: child's Div
        let child_div = frame.node(child_entity).first_child.unwrap();
        // 5: "child"
        let child_text = frame.node(child_div).first_child.unwrap();

        assert_eq!(node_text(&frame, child_text), "Child");
    }

    #[test]
    fn releases_entity_borrows_after_rendering() {
        let entities = EntityArena::<2048, 16>::default();
        let listeners = ListenerArena::<2048, 16>::default();
        let mut frame = FrameArena::<32, 256>::default();
        let notified = Cell::new(false);

        let child = entities.insert(Child).unwrap();
        let parent = entities.insert(Parent { child }).unwrap();

        frame.mount(parent).unwrap();
        frame
            .expand_entities(&entities, &listeners, &notified)
            .unwrap();

        // if rendering leaked the exclusive borrow, either of these would return BorrowConflict.
        entities.update(parent, |_parent| {}).unwrap();
        entities.update(child, |_child| {}).unwrap();
    }

    struct Label {
        text: String,
    }

    impl Render for Label {
        fn render<'a>(&'a mut self, _cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
            div().child(self.text.as_str())
        }
    }

    #[test]
    fn copies_entity_text_into_frame_storage() {
        let entities = EntityArena::<2048, 16>::default();
        let listeners = ListenerArena::<2048, 16>::default();
        let mut frame = FrameArena::<32, 256>::default();
        let notified = Cell::new(false);

        let label = entities
            .insert(Label {
                text: String::from("Persistent state text"),
            })
            .unwrap();

        let root = frame.mount(label).unwrap();
        frame
            .expand_entities(&entities, &listeners, &notified)
            .unwrap();

        // mutate the original persistent state after the frame has already been built.
        entities
            .update(label, |label| {
                label.text.clear();
                label.text.push_str("Changed");
            })
            .unwrap();

        let label_div = frame.node(root).first_child.unwrap();
        let text_node = frame.node(label_div).first_child.unwrap();

        assert_eq!(node_text(&frame, text_node,), "Persistent state text");
    }

    struct DuplicateParent {
        child: Entity<Child>,
    }

    impl Render for DuplicateParent {
        fn render<'a>(&'a mut self, _cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
            div().child(self.child).child(self.child)
        }
    }

    #[test]
    fn rejects_duplicate_entity_mounts() {
        let entities = EntityArena::<2048, 16>::default();
        let listeners = ListenerArena::<2048, 16>::default();
        let mut frame = FrameArena::<32, 256>::default();
        let notified = Cell::new(false);

        let child = entities.insert(Child).unwrap();
        let parent = entities.insert(DuplicateParent { child }).unwrap();

        frame.mount(parent).unwrap();

        let result = frame.expand_entities(&entities, &listeners, &notified);

        assert_eq!(
            result,
            Err(MountError::DuplicateEntityMount(child.entity_id()))
        );
    }

    #[test]
    fn expanding_entities_twice_does_not_duplicate_nodes() {
        let entities = EntityArena::<2048, 16>::default();
        let listeners = ListenerArena::<2048, 16>::default();
        let mut frame = FrameArena::<32, 256>::default();
        let notified = Cell::new(false);

        let child = entities.insert(Child).unwrap();
        let parent = entities.insert(Parent { child }).unwrap();

        frame.mount(parent).unwrap();
        frame
            .expand_entities(&entities, &listeners, &notified)
            .unwrap();

        let first_count = frame.node_count();

        frame
            .expand_entities(&entities, &listeners, &notified)
            .unwrap();

        assert_eq!(frame.node_count(), first_count);
        assert_eq!(first_count, 6);
    }

    struct Counter {
        value: i32,
    }

    impl Counter {
        fn increment(&mut self, _: &ClickEvent, cx: &mut Context<Self>) {
            self.value += 1;
            cx.notify();
        }
    }

    impl Render for Counter {
        fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
            div().child(
                div()
                    .id("increment")
                    .on_click(cx.listener(Self::increment))
                    .child("+"),
            )
        }
    }

    #[test]
    fn entity_render_mounts_click_listener() {
        let entities = EntityArena::<2048, 16>::default();
        let listeners = ListenerArena::<2048, 16>::default();
        let mut frame = FrameArena::<32, 256>::default();
        let notified = Cell::new(false);

        let counter = entities.insert(Counter { value: 0 }).unwrap();

        frame.mount(counter).unwrap();
        frame
            .expand_entities(&entities, &listeners, &notified)
            .unwrap();

        let button =
            find_element(&frame, ElementId::Name("increment")).expect("increment element missing");

        assert!(frame.node(button).interaction.click.is_some());
    }

    #[test]
    fn mounted_listener_updates_owning_entity() {
        let entities = EntityArena::<2048, 16>::default();
        let listeners = ListenerArena::<2048, 16>::default();
        let mut frame = FrameArena::<32, 256>::default();
        let notified = Cell::new(false);

        let counter = entities.insert(Counter { value: 0 }).unwrap();

        frame.mount(counter).unwrap();
        frame
            .expand_entities(&entities, &listeners, &notified)
            .unwrap();

        let button =
            find_element(&frame, ElementId::Name("increment")).expect("increment element missing");

        let listener_id = frame
            .node(button)
            .interaction
            .click
            .expect("click listener missing");

        let listener = Listener::<ClickEvent>::from_id(listener_id);
        listeners
            .invoke(listener, &ClickEvent, &entities, &notified)
            .unwrap();

        let value = entities.read(counter, |counter| counter.value);

        assert_eq!(value, Ok(1));
        assert!(notified.get());
    }

    struct Status {
        value: i32,
    }

    struct Controller {
        status: Entity<Status>,
    }

    impl Controller {
        fn update_status(&mut self, _: &ClickEvent, cx: &mut Context<Self>) {
            self.status
                .update(cx, |status, cx| {
                    status.value += 10;
                    cx.notify();
                })
                .unwrap();
        }
    }

    impl Render for Controller {
        fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
            div().child(
                div()
                    .id("update-status")
                    .on_click(cx.listener(Self::update_status))
                    .child("Update"),
            )
        }
    }

    #[test]
    fn rendered_listener_can_update_another_entity() {
        let entities = EntityArena::<2048, 16>::default();
        let listeners = ListenerArena::<2048, 16>::default();
        let mut frame = FrameArena::<32, 256>::default();
        let notified = Cell::new(false);

        let status = entities.insert(Status { value: 0 }).unwrap();
        let controller = entities.insert(Controller { status }).unwrap();

        frame.mount(controller).unwrap();
        frame
            .expand_entities(&entities, &listeners, &notified)
            .unwrap();

        let button = find_element(&frame, ElementId::Name("update-status")).unwrap();

        let listener_id = frame.node(button).interaction.click.unwrap();
        let listener = Listener::<ClickEvent>::from_id(listener_id);
        listeners
            .invoke(listener, &ClickEvent, &entities, &notified)
            .unwrap();

        assert_eq!(entities.read(status, |status| status.value,), Ok(10));
        assert!(notified.get());
    }

    #[test]
    fn entity_render_access_errors_become_mount_errors() {
        let entities = EntityArena::<1024, 8>::default();
        let listeners = ListenerArena::<1024, 8>::default();
        let mut frame = FrameArena::<16, 128>::default();
        let notified = Cell::new(false);

        let child = entities.insert(Child).unwrap();
        let fake = Entity::<Parent>::from_id(child.entity_id());

        frame.mount(fake).unwrap();

        let result = frame.expand_entities(&entities, &listeners, &notified);

        assert_eq!(
            result,
            Err(MountError::EntityAccess(EntityAccessError::TypeMismatch))
        );
    }

    #[test]
    fn expanded_entity_root_is_child_of_entity_node() {
        let entities = EntityArena::<1024, 8>::default();
        let listeners = ListenerArena::<1024, 8>::default();
        let mut frame = FrameArena::<16, 128>::default();
        let notified = Cell::new(false);

        let child = entities.insert(Child).unwrap();
        let entity_node = frame.mount(child).unwrap();

        frame
            .expand_entities(&entities, &listeners, &notified)
            .unwrap();

        let rendered_root = frame.node(entity_node).first_child.unwrap();

        assert_eq!(frame.node(rendered_root).parent, Some(entity_node));
        assert_eq!(frame.node(rendered_root).next_sibling, None);
    }

    struct StableApp;

    impl Render for StableApp {
        fn render<'a>(&'a mut self, _cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
            div().child(div().id("button").child("Press"))
        }
    }

    #[test]
    fn element_identity_is_stable_across_frames() {
        let entities = EntityArena::<2048, 16>::default();
        let listeners = ListenerArena::<1024, 16>::default();
        let mut states = ElementStateTable::<32>::default();
        let mut frame = FrameArena::<32, 256>::default();
        let notified = Cell::new(false);

        let app = entities.insert(StableApp).unwrap();

        build_frame(
            &mut frame,
            &mut states,
            app,
            &entities,
            &listeners,
            &notified,
            1,
        )
        .unwrap();

        let button = find_element(&frame, ElementId::Name("button")).unwrap();
        let first = state_id(&frame, button);

        build_frame(
            &mut frame,
            &mut states,
            app,
            &entities,
            &listeners,
            &notified,
            2,
        )
        .unwrap();

        let button = find_element(&frame, ElementId::Name("button")).unwrap();
        let second = state_id(&frame, button);

        assert_eq!(first, second);
        assert!(states.contains(first));
        assert_eq!(states.len(), 1);
    }

    struct WrapperApp {
        _wrapped: bool,
    }

    impl Render for WrapperApp {
        fn render<'a>(&'a mut self, _cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
            // we can't return two unrelated concrete types from a normal if,
            // so keep the same outer type and conditionally insert unnamed
            // wrappers in separate test component types below.
            div().id("panel").child(div().id("button"))
        }
    }

    struct WrappedApp;

    impl Render for WrappedApp {
        fn render<'a>(&'a mut self, _cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
            div()
                .id("panel")
                .child(div().child(div().child(div().id("button"))))
        }
    }

    #[test]
    fn unnamed_wrappers_do_not_affect_identity_path() {
        let entities = EntityArena::<4096, 16>::default();
        let listeners = ListenerArena::<1024, 16>::default();
        let mut states = ElementStateTable::<32>::default();
        let mut frame = FrameArena::<64, 256>::default();
        let notified = Cell::new(false);

        let plain = entities.insert(WrapperApp { _wrapped: false }).unwrap();
        let wrapped = entities.insert(WrappedApp).unwrap();

        // these are different entities, so their Ids should NOT be equal.
        // what we're validating here is the parent relationship inside each
        // tree: unnamed wrappers do not become identity parents.

        build_frame(
            &mut frame,
            &mut states,
            plain,
            &entities,
            &listeners,
            &notified,
            1,
        )
        .unwrap();

        let panel = find_element(&frame, ElementId::Name("panel")).unwrap();
        let button = find_element(&frame, ElementId::Name("button")).unwrap();
        let panel_state = state_id(&frame, panel);
        let button_state = state_id(&frame, button);
        let button_entry = states.entry(button_state).unwrap();

        assert_eq!(
            button_entry.key.parent,
            IdentityParent::Element(panel_state),
        );

        build_frame(
            &mut frame,
            &mut states,
            wrapped,
            &entities,
            &listeners,
            &notified,
            2,
        )
        .unwrap();

        let panel = find_element(&frame, ElementId::Name("panel")).unwrap();
        let button = find_element(&frame, ElementId::Name("button")).unwrap();
        let panel_state = state_id(&frame, panel);
        let button_state = state_id(&frame, button);
        let button_entry = states.entry(button_state).unwrap();

        assert_eq!(
            button_entry.key.parent,
            IdentityParent::Element(panel_state),
        );
    }

    struct NestedApp;

    impl Render for NestedApp {
        fn render<'a>(&'a mut self, _cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
            div().id("panel").child(div().id("button"))
        }
    }

    #[test]
    fn nested_element_uses_nearest_identified_parent() {
        let entities = EntityArena::<2048, 16>::default();
        let listeners = ListenerArena::<1024, 16>::default();
        let mut states = ElementStateTable::<32>::default();
        let mut frame = FrameArena::<32, 256>::default();
        let notified = Cell::new(false);

        let app = entities.insert(NestedApp).unwrap();

        build_frame(
            &mut frame,
            &mut states,
            app,
            &entities,
            &listeners,
            &notified,
            1,
        )
        .unwrap();

        let panel = find_element(&frame, ElementId::Name("panel")).unwrap();
        let button = find_element(&frame, ElementId::Name("button")).unwrap();
        let panel_state = state_id(&frame, panel);
        let button_state = state_id(&frame, button);

        assert_ne!(panel_state, button_state);

        let panel_entry = states.entry(panel_state).unwrap();

        assert_eq!(
            panel_entry.key.parent,
            IdentityParent::Entity(app.entity_id()),
        );

        let button_entry = states.entry(button_state).unwrap();

        assert_eq!(
            button_entry.key.parent,
            IdentityParent::Element(panel_state),
        );
    }

    struct DuplicateApp;

    impl Render for DuplicateApp {
        fn render<'a>(&'a mut self, _cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
            div().child(div().id("button")).child(div().id("button"))
        }
    }

    #[test]
    fn duplicate_ids_in_same_scope_are_rejected() {
        let entities = EntityArena::<2048, 16>::default();
        let listeners = ListenerArena::<1024, 16>::default();
        let mut states = ElementStateTable::<32>::default();
        let mut frame = FrameArena::<32, 256>::default();
        let notified = Cell::new(false);

        let app = entities.insert(DuplicateApp).unwrap();

        frame.mount(app).unwrap();
        frame
            .expand_entities(&entities, &listeners, &notified)
            .unwrap();

        let result = frame.resolve_identities(&mut states, 1);

        assert_eq!(
            result,
            Err(IdentityError::DuplicateElementId {
                id: ElementId::Name("button"),
            })
        );
    }

    struct SeparateScopesApp;

    impl Render for SeparateScopesApp {
        fn render<'a>(&'a mut self, _cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
            div()
                .child(div().id("left").child(div().id("button")))
                .child(div().id("right").child(div().id("button")))
        }
    }

    #[test]
    fn same_local_id_is_allowed_under_different_identified_parents() {
        let entities = EntityArena::<2048, 16>::default();
        let listeners = ListenerArena::<1024, 16>::default();
        let mut states = ElementStateTable::<32>::default();
        let mut frame = FrameArena::<32, 256>::default();
        let notified = Cell::new(false);

        let app = entities.insert(SeparateScopesApp).unwrap();

        build_frame(
            &mut frame,
            &mut states,
            app,
            &entities,
            &listeners,
            &notified,
            1,
        )
        .unwrap();

        let buttons = find_all_elements(&frame, ElementId::Name("button"));

        assert_eq!(buttons.len(), 2);

        let first = state_id(&frame, buttons[0]);
        let second = state_id(&frame, buttons[1]);

        assert_ne!(first, second);

        let first_parent = states.entry(first).unwrap().key.parent;
        let second_parent = states.entry(second).unwrap().key.parent;

        assert_ne!(first_parent, second_parent);
    }

    struct Widget;

    impl Render for Widget {
        fn render<'a>(&'a mut self, _cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
            div().id("button").child("Button")
        }
    }

    struct WidgetsApp {
        left: Entity<Widget>,
        right: Entity<Widget>,
    }

    impl Render for WidgetsApp {
        fn render<'a>(&'a mut self, _cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
            div().child(self.left).child(self.right)
        }
    }

    #[test]
    fn separate_entities_have_separate_identity_namespaces() {
        let entities = EntityArena::<4096, 16>::default();
        let listeners = ListenerArena::<1024, 16>::default();
        let mut states = ElementStateTable::<32>::default();
        let mut frame = FrameArena::<64, 256>::default();
        let notified = Cell::new(false);

        let left = entities.insert(Widget).unwrap();
        let right = entities.insert(Widget).unwrap();
        let app = entities.insert(WidgetsApp { left, right }).unwrap();

        build_frame(
            &mut frame,
            &mut states,
            app,
            &entities,
            &listeners,
            &notified,
            1,
        )
        .unwrap();

        let buttons = find_all_elements(&frame, ElementId::Name("button"));

        assert_eq!(buttons.len(), 2);

        let a = state_id(&frame, buttons[0]);
        let b = state_id(&frame, buttons[1]);

        assert_ne!(a, b);

        let a_parent = states.entry(a).unwrap().key.parent;
        let b_parent = states.entry(b).unwrap().key.parent;

        assert!(matches!(a_parent, IdentityParent::Entity(_)));
        assert!(matches!(b_parent, IdentityParent::Entity(_)));
        assert_ne!(a_parent, b_parent);

        let expected = [
            IdentityParent::Entity(left.entity_id()),
            IdentityParent::Entity(right.entity_id()),
        ];

        assert!(expected.contains(&a_parent));
        assert!(expected.contains(&b_parent));
    }

    struct MovableChild;

    impl Render for MovableChild {
        fn render<'a>(&'a mut self, _cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
            div().id("internal-button").child("Internal")
        }
    }

    struct ParentA {
        child: Entity<MovableChild>,
    }

    impl Render for ParentA {
        fn render<'a>(&'a mut self, _cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
            div().id("left").child(self.child)
        }
    }

    struct ParentB {
        child: Entity<MovableChild>,
    }

    impl Render for ParentB {
        fn render<'a>(&'a mut self, _cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
            div()
                .id("right")
                .child(div().child(div().child(self.child)))
        }
    }

    #[test]
    fn moving_entity_does_not_change_internal_element_identity() {
        let entities = EntityArena::<4096, 16>::default();
        let listeners = ListenerArena::<1024, 16>::default();
        let mut states = ElementStateTable::<32>::default();
        let mut frame = FrameArena::<64, 256>::default();
        let notified = Cell::new(false);

        let child = entities.insert(MovableChild).unwrap();
        let parent_a = entities.insert(ParentA { child }).unwrap();
        let parent_b = entities.insert(ParentB { child }).unwrap();

        build_frame(
            &mut frame,
            &mut states,
            parent_a,
            &entities,
            &listeners,
            &notified,
            1,
        )
        .unwrap();

        let internal = find_element(&frame, ElementId::Name("internal-button")).unwrap();
        let first = state_id(&frame, internal);
        let first_parent = states.entry(first).unwrap().key.parent;

        assert_eq!(first_parent, IdentityParent::Entity(child.entity_id()));

        build_frame(
            &mut frame,
            &mut states,
            parent_b,
            &entities,
            &listeners,
            &notified,
            2,
        )
        .unwrap();

        let internal = find_element(&frame, ElementId::Name("internal-button")).unwrap();
        let second = state_id(&frame, internal);

        assert_eq!(first, second);

        let second_parent = states.entry(second).unwrap().key.parent;

        assert_eq!(second_parent, IdentityParent::Entity(child.entity_id()));
    }

    struct WithButton;

    impl Render for WithButton {
        fn render<'a>(&'a mut self, _cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
            div().child(div().id("button"))
        }
    }

    struct WithoutButton;

    impl Render for WithoutButton {
        fn render<'a>(&'a mut self, _cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
            div().child("Nothing")
        }
    }

    #[test]
    fn element_state_is_removed_when_element_disappears() {
        let entities = EntityArena::<4096, 16>::default();
        let listeners = ListenerArena::<1024, 16>::default();
        let mut states = ElementStateTable::<32>::default();
        let mut frame = FrameArena::<32, 256>::default();
        let notified = Cell::new(false);

        let with_button = entities.insert(WithButton).unwrap();
        let without_button = entities.insert(WithoutButton).unwrap();

        build_frame(
            &mut frame,
            &mut states,
            with_button,
            &entities,
            &listeners,
            &notified,
            1,
        )
        .unwrap();

        let button = find_element(&frame, ElementId::Name("button")).unwrap();
        let old = state_id(&frame, button);

        assert!(states.contains(old));

        build_frame(
            &mut frame,
            &mut states,
            without_button,
            &entities,
            &listeners,
            &notified,
            2,
        )
        .unwrap();
        states.sweep(2);

        assert!(!states.contains(old));
    }

    #[test]
    fn reappearing_element_gets_new_generation() {
        let entities = EntityArena::<4096, 16>::default();
        let listeners = ListenerArena::<1024, 16>::default();
        let mut states = ElementStateTable::<32>::default();
        let mut frame = FrameArena::<32, 256>::default();
        let notified = Cell::new(false);

        let with_button = entities.insert(WithButton).unwrap();
        let without_button = entities.insert(WithoutButton).unwrap();

        // frame 1: button exists.
        build_frame(
            &mut frame,
            &mut states,
            with_button,
            &entities,
            &listeners,
            &notified,
            1,
        )
        .unwrap();

        let button = find_element(&frame, ElementId::Name("button")).unwrap();
        let old = state_id(&frame, button);

        // frame 2: button disappears and is swept.
        build_frame(
            &mut frame,
            &mut states,
            without_button,
            &entities,
            &listeners,
            &notified,
            2,
        )
        .unwrap();

        assert!(!states.contains(old));

        // frame 3: button comes back.
        build_frame(
            &mut frame,
            &mut states,
            with_button,
            &entities,
            &listeners,
            &notified,
            3,
        )
        .unwrap();

        let button = find_element(&frame, ElementId::Name("button")).unwrap();
        let new = state_id(&frame, button);

        assert_ne!(old, new);

        // with a first-free-slot allocator this will normally be the same slot with
        // a newer generation.
        assert_eq!(old.slot(), new.slot());

        assert_ne!(old.generation(), new.generation());
    }

    struct TooManyStatesApp;

    impl Render for TooManyStatesApp {
        fn render<'a>(&'a mut self, _cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
            div().child(div().id("a")).child(div().id("b"))
        }
    }

    #[test]
    fn identity_resolution_reports_state_capacity_exhaustion() {
        let entities = EntityArena::<2048, 16>::default();
        let listeners = ListenerArena::<1024, 16>::default();
        let mut states = ElementStateTable::<1>::default();
        let mut frame = FrameArena::<16, 128>::default();
        let notified = Cell::new(false);

        let app = entities.insert(TooManyStatesApp).unwrap();

        frame.mount(app).unwrap();
        frame
            .expand_entities(&entities, &listeners, &notified)
            .unwrap();

        assert_eq!(
            frame.resolve_identities(&mut states, 1,),
            Err(IdentityError::StatesFull)
        );
    }

    #[test]
    fn failed_identity_resolution_can_be_aborted() {
        let mut states = ElementStateTable::<8>::default();

        let parent = IdentityParent::Entity(EntityId::new(0, 0));
        let old = states.resolve(parent, ElementId::Name("old"), 1).unwrap();

        states.sweep(1);

        assert!(states.contains(old));

        states.resolve(parent, ElementId::Name("new"), 2).unwrap();
        states.resolve(parent, ElementId::Name("old"), 2).unwrap();
        states.abort_frame(2);

        assert!(states.contains(old));
        assert_eq!(states.len(), 1);
    }

    #[test]
    fn plain_string_mounts_without_text_style_overrides() {
        let mut frame = FrameArena::<8, 64>::default();

        let node = frame.mount("Hello").unwrap();

        assert_eq!(frame.node(node).text_style, TextStylePatch::default());
        assert_eq!(node_text(&frame, node), "Hello");
    }

    #[test]
    fn explicit_text_element_preserves_text_style_overrides() {
        let mut frame = FrameArena::<8, 64>::default();

        let node = frame
            .mount(
                text("Hello")
                    .font(FontId::new(2))
                    .text_color(Color::RED)
                    .line_height(px(18)),
            )
            .unwrap();

        assert_eq!(node_text(&frame, node), "Hello");
        assert_eq!(frame.node(node).text_style.font(), Some(FontId::new(2)));
        assert_eq!(frame.node(node).text_style.color(), Some(Color::RED));
        assert_eq!(frame.node(node).text_style.line_height(), Some(px(18)));
    }

    #[test]
    fn text_style_resolves_through_nested_elements() {
        let mut frame = FrameArena::<16, 128>::default();

        let root = frame
            .mount(
                div()
                    .font(FontId::new(1))
                    .text_color(Color::WHITE)
                    .line_height(px(14))
                    .child(
                        div()
                            .text_color(Color::RED)
                            .child(text("Hello").font(FontId::new(2))),
                    ),
            )
            .unwrap();

        frame.resolve_text_styles(root);

        let inner = frame.node(root).first_child.unwrap();
        let text_node = frame.node(inner).first_child.unwrap();

        assert_eq!(
            frame.node(text_node).effective_text_style,
            TextStyle {
                font: FontId::new(2),
                color: Color::RED,
                line_height: Some(px(14)),
            }
        );
    }
}
