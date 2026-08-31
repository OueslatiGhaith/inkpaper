    use core::{any::TypeId, cell::Cell};
    use std::string::String;

    use crate::{
        callback_store::CallbackArena,
        element_state::{ElementStateId, ElementStateTable, IdentityError, IdentityParent},
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
        const CALLBACK_BYTES: usize,
        const CALLBACK_SLOTS: usize,
        const GLOBAL_BYTES: usize,
        const GLOBAL_SLOTS: usize,
    >(
        frame: &mut FrameArena<NODES, TEXT_BYTES>,
        states: &mut ElementStateTable<STATES>,
        root: Entity<Root>,
        entities: &EntityArena<ENTITY_BYTES, ENTITY_SLOTS>,
        globals: &GlobalArena<GLOBAL_BYTES, GLOBAL_SLOTS>,
        callbacks: &CallbackArena<CALLBACK_BYTES, CALLBACK_SLOTS>,
        notified: &Cell<bool>,
        generation: u32,
    ) -> Result<NodeId, IdentityError>
    where
        Root: Render,
    {
        frame.clear();

        let app = AppContext::from_globals(globals);
        let root = frame.mount(root, app).expect("mount should succeed");

        frame
            .expand_entities(entities, globals, callbacks, notified)
            .expect("entity expansion should succeed");

        frame.resolve_identities(states, generation)?;
        states.sweep(generation);

        Ok(root)
    }

    #[test]
    fn mounts_div_tree() {
        let mut frame = FrameArena::<16, 128>::default();
        let globals = GlobalArena::<0, 0>::default();
        let app = AppContext::from_globals(&globals);
        let root = frame.mount(div().child("A").child("B"), app).unwrap();

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
        let globals = GlobalArena::<0, 0>::default();
        let app = AppContext::from_globals(&globals);

        let root = frame
            .mount(div().child("A").child("B").child("C"), app)
            .unwrap();
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
        let globals = GlobalArena::<0, 0>::default();
        let app = AppContext::from_globals(&globals);

        let root = frame
            .mount(div().child("Top").child(div().child("Nested")), app)
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
        let globals = GlobalArena::<0, 0>::default();
        let app = AppContext::from_globals(&globals);

        let root = frame
            .mount(div().flex().flex_col().w_full().p(px(8)), app)
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
        let globals = GlobalArena::<0, 0>::default();
        let app = AppContext::from_globals(&globals);

        let root = {
            let text = String::from("Hello");
            frame.mount(div().child(text.as_str()), app).unwrap()
            // `text` is dropped at the end of this block.
        };

        let text_node = frame.node(root).first_child.unwrap();

        assert_eq!(node_text(&frame, text_node), "Hello");
        assert_eq!(frame.text_bytes_used(), 5);
    }

    #[test]
    fn mounts_stateful_element_identity() {
        let mut frame = FrameArena::<8, 128>::default();
        let globals = GlobalArena::<0, 0>::default();
        let app = AppContext::from_globals(&globals);

        let button = frame.mount(div().id("button").child("Press"), app).unwrap();

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
        let globals = GlobalArena::<0, 0>::default();
        let app = AppContext::from_globals(&globals);

        // required nodes:
        // 0 Div
        // 1 Text "A"
        // 2 Text "B"
        // capacity is only 2.

        let result = frame.mount(div().child("A").child("B"), app);

        assert_eq!(result, Err(MountError::NodesFull));
    }

    #[test]
    fn reports_text_storage_exhaustion() {
        let mut frame = FrameArena::<8, 4>::default();
        let globals = GlobalArena::<0, 0>::default();
        let app = AppContext::from_globals(&globals);

        // "Hello" needs 5 UTF-8 bytes, but the frame only has 4.
        let result = frame.mount(div().child("Hello"), app);

        assert_eq!(result, Err(MountError::TextStorageFull));
    }

    #[test]
    fn clear_resets_frame_storage() {
        let mut frame = FrameArena::<8, 128>::default();
        let globals = GlobalArena::<0, 0>::default();
        let app = AppContext::from_globals(&globals);

        frame.mount(div().child("Hello"), app).unwrap();

        assert_eq!(frame.node_count(), 2);
        assert_eq!(frame.text_bytes_used(), 5);

        frame.clear();

        assert_eq!(frame.node_count(), 0);
        assert_eq!(frame.text_bytes_used(), 0);

        // verify it is usable again after clearing.
        let root = frame.mount(div().child("Again"), app).unwrap();

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
        let globals = GlobalArena::<0, 0>::default();
        let app = AppContext::from_globals(&globals);

        // we don't need a real EntityArena for this test yet because mounting an
        // Entity<T> only stores its EntityId.
        let child = Entity::<Child>::from_id(EntityId::new(7, 0));
        let root = frame.mount(div().child(child), app).unwrap();

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
        let callbacks = CallbackArena::<2048, 16>::default();
        let globals = GlobalArena::<0, 0>::default();
        let app = AppContext::from_globals(&globals);
        let notified = Cell::new(false);

        let child = entities.insert(Child).unwrap();
        let parent = entities.insert(Parent { child }).unwrap();
        let root = frame.mount(parent, app).unwrap();

        frame
            .expand_entities(&entities, &globals, &callbacks, &notified)
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
        let callbacks = CallbackArena::<2048, 16>::default();
        let globals = GlobalArena::<0, 0>::default();
        let app = AppContext::from_globals(&globals);
        let mut frame = FrameArena::<32, 256>::default();
        let notified = Cell::new(false);

        let child = entities.insert(Child).unwrap();
        let parent = entities.insert(Parent { child }).unwrap();

        frame.mount(parent, app).unwrap();
        frame
            .expand_entities(&entities, &globals, &callbacks, &notified)
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
        let callbacks = CallbackArena::<2048, 16>::default();
        let globals = GlobalArena::<0, 0>::default();
        let app = AppContext::from_globals(&globals);
        let mut frame = FrameArena::<32, 256>::default();
        let notified = Cell::new(false);

        let label = entities
            .insert(Label {
                text: String::from("Persistent state text"),
            })
            .unwrap();

        let root = frame.mount(label, app).unwrap();
        frame
            .expand_entities(&entities, &globals, &callbacks, &notified)
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
        let callbacks = CallbackArena::<2048, 16>::default();
        let globals = GlobalArena::<0, 0>::default();
        let app = AppContext::from_globals(&globals);
        let mut frame = FrameArena::<32, 256>::default();
        let notified = Cell::new(false);

        let child = entities.insert(Child).unwrap();
        let parent = entities.insert(DuplicateParent { child }).unwrap();

        frame.mount(parent, app).unwrap();

        let result = frame.expand_entities(&entities, &globals, &callbacks, &notified);

        assert_eq!(
            result,
            Err(MountError::DuplicateEntityMount(child.entity_id()))
        );
    }

    #[test]
    fn expanding_entities_twice_does_not_duplicate_nodes() {
        let entities = EntityArena::<2048, 16>::default();
        let callbacks = CallbackArena::<2048, 16>::default();
        let globals = GlobalArena::<0, 0>::default();
        let app = AppContext::from_globals(&globals);
        let mut frame = FrameArena::<32, 256>::default();
        let notified = Cell::new(false);

        let child = entities.insert(Child).unwrap();
        let parent = entities.insert(Parent { child }).unwrap();

        frame.mount(parent, app).unwrap();
        frame
            .expand_entities(&entities, &globals, &callbacks, &notified)
            .unwrap();

        let first_count = frame.node_count();

        frame
            .expand_entities(&entities, &globals, &callbacks, &notified)
            .unwrap();

        assert_eq!(frame.node_count(), first_count);
        assert_eq!(first_count, 6);
    }

    struct Counter {
        value: i32,
    }

    impl Counter {
        fn increment(&mut self, _: &ActivateEvent, cx: &mut Context<Self>) {
            self.value += 1;
            cx.notify();
        }
    }

    impl Render for Counter {
        fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
            div().child(
                div()
                    .id("increment")
                    .on_activate(cx.listener(Self::increment))
                    .child("+"),
            )
        }
    }

    #[test]
    fn entity_render_mounts_click_listener() {
        let entities = EntityArena::<2048, 16>::default();
        let callbacks = CallbackArena::<2048, 16>::default();
        let globals = GlobalArena::<0, 0>::default();
        let app = AppContext::from_globals(&globals);
        let mut frame = FrameArena::<32, 256>::default();
        let notified = Cell::new(false);

        let counter = entities.insert(Counter { value: 0 }).unwrap();

        frame.mount(counter, app).unwrap();
        frame
            .expand_entities(&entities, &globals, &callbacks, &notified)
            .unwrap();

        let button =
            find_element(&frame, ElementId::Name("increment")).expect("increment element missing");

        assert!(frame.node(button).interaction.focusable);
    }

    #[test]
    fn mounted_listener_updates_owning_entity() {
        let entities = EntityArena::<2048, 16>::default();
        let callbacks = CallbackArena::<2048, 16>::default();
        let globals = GlobalArena::<0, 0>::default();
        let app = AppContext::from_globals(&globals);
        let mut frame = FrameArena::<32, 256>::default();
        let notified = Cell::new(false);

        let counter = entities.insert(Counter { value: 0 }).unwrap();

        frame.mount(counter, app).unwrap();
        frame
            .expand_entities(&entities, &globals, &callbacks, &notified)
            .unwrap();

        let button =
            find_element(&frame, ElementId::Name("increment")).expect("increment element missing");

        let listener_id = frame
            .event_callbacks(button, TypeId::of::<ActivateEvent>())
            .next()
            .expect("activation listener missing");

        let listener = Listener::<ActivateEvent>::from_id(listener_id);
        callbacks
            .invoke_listener(listener, &ActivateEvent, &entities, &globals, &notified)
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
        fn update_status(&mut self, _: &ActivateEvent, cx: &mut Context<Self>) {
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
                    .on_activate(cx.listener(Self::update_status))
                    .child("Update"),
            )
        }
    }

    #[test]
    fn rendered_listener_can_update_another_entity() {
        let entities = EntityArena::<2048, 16>::default();
        let callbacks = CallbackArena::<2048, 16>::default();
        let globals = GlobalArena::<0, 0>::default();
        let app = AppContext::from_globals(&globals);
        let mut frame = FrameArena::<32, 256>::default();
        let notified = Cell::new(false);

        let status = entities.insert(Status { value: 0 }).unwrap();
        let controller = entities.insert(Controller { status }).unwrap();

        frame.mount(controller, app).unwrap();
        frame
            .expand_entities(&entities, &globals, &callbacks, &notified)
            .unwrap();

        let button = find_element(&frame, ElementId::Name("update-status")).unwrap();

        let listener_id = frame
            .event_callbacks(button, core::any::TypeId::of::<ActivateEvent>())
            .next()
            .expect("activation listener missing");
        let listener = Listener::<ActivateEvent>::from_id(listener_id);
        callbacks
            .invoke_listener(listener, &ActivateEvent, &entities, &globals, &notified)
            .unwrap();

        assert_eq!(entities.read(status, |status| status.value,), Ok(10));
        assert!(notified.get());
    }

    #[test]
    fn entity_render_access_errors_become_mount_errors() {
        let entities = EntityArena::<1024, 8>::default();
        let callbacks = CallbackArena::<1024, 8>::default();
        let globals = GlobalArena::<0, 0>::default();
        let app = AppContext::from_globals(&globals);
        let mut frame = FrameArena::<16, 128>::default();
        let notified = Cell::new(false);

        let child = entities.insert(Child).unwrap();
        let fake = Entity::<Parent>::from_id(child.entity_id());

        frame.mount(fake, app).unwrap();

        let result = frame.expand_entities(&entities, &globals, &callbacks, &notified);

        assert_eq!(
            result,
            Err(MountError::EntityAccess(EntityAccessError::TypeMismatch))
        );
    }

    #[test]
    fn expanded_entity_root_is_child_of_entity_node() {
        let entities = EntityArena::<1024, 8>::default();
        let callbacks = CallbackArena::<1024, 8>::default();
        let globals = GlobalArena::<0, 0>::default();
        let app = AppContext::from_globals(&globals);
        let mut frame = FrameArena::<16, 128>::default();
        let notified = Cell::new(false);

        let child = entities.insert(Child).unwrap();
        let entity_node = frame.mount(child, app).unwrap();

        frame
            .expand_entities(&entities, &globals, &callbacks, &notified)
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
        let callbacks = CallbackArena::<1024, 16>::default();
        let mut states = ElementStateTable::<32>::default();
        let globals = GlobalArena::<0, 0>::default();
        let mut frame = FrameArena::<32, 256>::default();
        let notified = Cell::new(false);

        let app = entities.insert(StableApp).unwrap();

        build_frame(
            &mut frame,
            &mut states,
            app,
            &entities,
            &globals,
            &callbacks,
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
            &globals,
            &callbacks,
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
        let callbacks = CallbackArena::<1024, 16>::default();
        let mut states = ElementStateTable::<32>::default();
        let globals = GlobalArena::<0, 0>::default();
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
            &globals,
            &callbacks,
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
            &globals,
            &callbacks,
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
        let callbacks = CallbackArena::<1024, 16>::default();
        let mut states = ElementStateTable::<32>::default();
        let globals = GlobalArena::<0, 0>::default();
        let mut frame = FrameArena::<32, 256>::default();
        let notified = Cell::new(false);

        let app = entities.insert(NestedApp).unwrap();

        build_frame(
            &mut frame,
            &mut states,
            app,
            &entities,
            &globals,
            &callbacks,
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
        let callbacks = CallbackArena::<1024, 16>::default();
        let mut states = ElementStateTable::<32>::default();
        let globals = GlobalArena::<0, 0>::default();
        let cx = AppContext::from_globals(&globals);
        let mut frame = FrameArena::<32, 256>::default();
        let notified = Cell::new(false);

        let app = entities.insert(DuplicateApp).unwrap();

        frame.mount(app, cx).unwrap();
        frame
            .expand_entities(&entities, &globals, &callbacks, &notified)
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
        let callbacks = CallbackArena::<1024, 16>::default();
        let mut states = ElementStateTable::<32>::default();
        let globals = GlobalArena::<0, 0>::default();
        let mut frame = FrameArena::<32, 256>::default();
        let notified = Cell::new(false);

        let app = entities.insert(SeparateScopesApp).unwrap();

        build_frame(
            &mut frame,
            &mut states,
            app,
            &entities,
            &globals,
            &callbacks,
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
        let callbacks = CallbackArena::<1024, 16>::default();
        let mut states = ElementStateTable::<32>::default();
        let globals = GlobalArena::<0, 0>::default();
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
            &globals,
            &callbacks,
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
        let callbacks = CallbackArena::<1024, 16>::default();
        let mut states = ElementStateTable::<32>::default();
        let globals = GlobalArena::<0, 0>::default();
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
            &globals,
            &callbacks,
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
            &globals,
            &callbacks,
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
        let callbacks = CallbackArena::<1024, 16>::default();
        let mut states = ElementStateTable::<32>::default();
        let globals = GlobalArena::<0, 0>::default();
        let mut frame = FrameArena::<32, 256>::default();
        let notified = Cell::new(false);

        let with_button = entities.insert(WithButton).unwrap();
        let without_button = entities.insert(WithoutButton).unwrap();

        build_frame(
            &mut frame,
            &mut states,
            with_button,
            &entities,
            &globals,
            &callbacks,
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
            &globals,
            &callbacks,
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
        let callbacks = CallbackArena::<1024, 16>::default();
        let mut states = ElementStateTable::<32>::default();
        let globals = GlobalArena::<0, 0>::default();
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
            &globals,
            &callbacks,
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
            &globals,
            &callbacks,
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
            &globals,
            &callbacks,
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
        let callbacks = CallbackArena::<1024, 16>::default();
        let mut states = ElementStateTable::<1>::default();
        let globals = GlobalArena::<0, 0>::default();
        let cx = AppContext::from_globals(&globals);
        let mut frame = FrameArena::<16, 128>::default();
        let notified = Cell::new(false);

        let app = entities.insert(TooManyStatesApp).unwrap();

        frame.mount(app, cx).unwrap();
        frame
            .expand_entities(&entities, &globals, &callbacks, &notified)
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
        let globals = GlobalArena::<0, 0>::default();
        let cx = AppContext::from_globals(&globals);

        let node = frame.mount("Hello", cx).unwrap();

        assert_eq!(frame.node(node).text_style, TextStyle::default());
        assert_eq!(node_text(&frame, node), "Hello");
    }

    #[test]
    fn explicit_text_element_preserves_text_style_overrides() {
        let mut frame = FrameArena::<8, 64>::default();
        let globals = GlobalArena::<0, 0>::default();
        let cx = AppContext::from_globals(&globals);

        let node = frame
            .mount(
                text("Hello")
                    .font(FontId::new(2))
                    .text_color(Color::RED)
                    .line_height(px(18)),
                cx,
            )
            .unwrap();

        assert_eq!(node_text(&frame, node), "Hello");
        assert_eq!(frame.node(node).text_style.font, Some(FontId::new(2)));
        assert_eq!(frame.node(node).text_style.color, Some(Color::RED));
        assert_eq!(
            frame.node(node).text_style.line_height,
            Some(LineHeight::Pixels(px(18)))
        );
    }

    #[test]
    fn text_style_resolves_through_nested_elements() {
        let mut frame = FrameArena::<16, 128>::default();
        let globals = GlobalArena::<0, 0>::default();
        let cx = AppContext::from_globals(&globals);

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
                cx,
            )
            .unwrap();

        frame.resolve_text_styles(root);

        let inner = frame.node(root).first_child.unwrap();
        let text_node = frame.node(inner).first_child.unwrap();

        assert_eq!(
            frame.node(text_node).effective_text_style,
            ResolvedTextStyle {
                font: FontId::new(2),
                color: Color::RED,
                line_height: LineHeight::Pixels(px(14)),
                ..Default::default()
            }
        );
    }

    fn node_image_source<const NODES: usize, const TEXT_BYTES: usize>(
        frame: &FrameArena<NODES, TEXT_BYTES>,
        node: NodeId,
    ) -> ImageSource {
        match frame.node(node).kind {
            NodeKind::Image { source, .. } => source,
            _ => panic!("expected image node"),
        }
    }

    #[test]
    fn mounts_image_leaf() {
        let source = ImageSource::new(ImageId::new(1), Size::new(px(16), px(12)));

        let mut frame = FrameArena::<8, 128>::default();
        let globals = GlobalArena::<0, 0>::default();
        let cx = AppContext::from_globals(&globals);

        let root = frame.mount(div().child(image(source)), cx).unwrap();

        assert_eq!(frame.node_count(), 2);

        let image_node = frame.node(root).first_child.unwrap();

        assert_eq!(node_image_source(&frame, image_node,), source);
        assert_eq!(frame.node(image_node).parent, Some(root));
        assert_eq!(frame.node(image_node).first_child, None);
    }
