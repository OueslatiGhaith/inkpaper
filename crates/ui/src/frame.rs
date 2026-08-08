use heapless::Vec;

use crate::{Element, ElementId, EntityId, IntoElement, ListenerId, StatefulInteractivity, Style};

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
    Div { style: Style },
    Text { text: TextRange },
    Entity { entity: EntityId },
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Node {
    pub(crate) kind: NodeKind,
    pub(crate) parent: Option<NodeId>,
    pub(crate) first_child: Option<NodeId>,
    pub(crate) last_child: Option<NodeId>,
    pub(crate) next_sibling: Option<NodeId>,
    pub(crate) element_id: Option<ElementId>,
    pub(crate) interaction: NodeInteraction,
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
            interaction: NodeInteraction::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct NodeInteraction {
    pub(crate) click: Option<ListenerId>,
}

impl From<StatefulInteractivity> for NodeInteraction {
    fn from(value: StatefulInteractivity) -> Self {
        Self { click: value.click }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MountError {
    NodesFull,
    TextStorageFull,
}

pub struct FrameArena<const NODES: usize, const TEXT_BYTES: usize> {
    nodes: Vec<Node, NODES>,
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

    pub(crate) fn text(&self, range: TextRange) -> &str {
        let bytes = &self.text[range.start..range.start + range.len];

        // bytes were copied from a valid str
        unsafe { core::str::from_utf8_unchecked(bytes) }
    }
}

impl<const NODES: usize, const TEXT_BYTES: usize> FrameStore for FrameArena<NODES, TEXT_BYTES> {
    fn push_div(&mut self, style: Style) -> Result<NodeId, MountError> {
        self.push_node(NodeKind::Div { style })
    }

    fn push_text(&mut self, text: &str) -> Result<NodeId, MountError> {
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

        self.push_node(NodeKind::Text { text: range })
    }

    fn push_entity(&mut self, entity: EntityId) -> Result<NodeId, MountError> {
        self.push_node(NodeKind::Entity { entity })
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
    fn push_text(&mut self, text: &str) -> Result<NodeId, MountError>;
    fn push_entity(&mut self, entity: EntityId) -> Result<NodeId, MountError>;
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

    pub(crate) fn push_text(&mut self, text: &str) -> Result<NodeId, MountError> {
        self.frame.push_text(text)
    }

    pub(crate) fn push_entity(&mut self, entity: EntityId) -> Result<NodeId, MountError> {
        self.frame.push_entity(entity)
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
    use heapless::String;

    use crate::*;

    fn node_text<const N: usize, const T: usize>(frame: &FrameArena<N, T>, id: NodeId) -> &str {
        match frame.node(id).kind {
            NodeKind::Text { text, .. } => frame.text(text),
            _ => panic!("expected text node"),
        }
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
            let text = String::<5>::try_from("Hello").unwrap();
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
            NodeKind::Entity { entity } => assert_eq!(entity, child.entity_id()),
            _ => panic!("expected entity placeholder node"),
        }
    }
}
