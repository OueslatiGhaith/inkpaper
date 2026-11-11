use core::{any::TypeId, cell::Cell};
use std::string::String;

use crate::pool::testing::{fill_leaving, pool_ref, pool_ref_of, pool_ref_with_room};
use crate::{
    callback::CallbackArena,
    element::state::{ElementStateId, ElementStateTable, IdentityError, IdentityParent},
    entity::EntityStore,
    *,
};

fn node_text(frame: &FrameArena, id: NodeId) -> &str {
    match frame.node(id).kind {
        NodeKind::Text { text, .. } => frame.text(text),
        _ => panic!("expected text node"),
    }
}

fn find_element(frame: &FrameArena, id: ElementId) -> Option<NodeId> {
    frame
        .node_ids()
        .find(|&node| frame.element_id(node) == Some(id))
}

fn find_all_elements(frame: &FrameArena, id: ElementId) -> std::vec::Vec<NodeId> {
    frame
        .node_ids()
        .filter(|&node| frame.element_id(node) == Some(id))
        .collect()
}

fn state_id(frame: &FrameArena, node: NodeId) -> ElementStateId {
    frame
        .element_state_id(node)
        .expect("element should have resolved state identity")
}

fn build_frame<Root>(
    frame: &mut FrameArena,
    states: &mut ElementStateTable,
    root: Entity<Root>,
    runtime: RuntimeCx<'_>,
    generation: u32,
) -> Result<NodeId, IdentityError>
where
    Root: Render,
{
    frame.clear();

    let app = AppContext::from_globals(runtime.globals);
    let root = frame.mount(root, app).expect("mount should succeed");

    frame
        .expand_entities(runtime)
        .expect("entity expansion should succeed");

    frame.resolve_identities(root, states, generation)?;
    states.sweep(generation);

    Ok(root)
}

#[test]
fn mounts_div_tree() {
    let mut frame = FrameArena::new_in(pool_ref());
    let globals = GlobalArena::new_in(pool_ref());
    let app = AppContext::from_globals(&globals);
    let root = frame.mount(div().child("A").child("B"), app).unwrap();

    assert_eq!(frame.node_count(), 3);

    let root_node = frame.node(root);
    let a = root_node.first_child.unwrap();
    let b = frame.node(a).next_sibling.unwrap();

    assert_eq!(frame.node(a).parent, Some(root));
    assert_eq!(frame.node(b).parent, Some(root));
    assert_eq!(frame.node(b).next_sibling, None);
    assert_eq!(frame.node(root).last_child, Some(b));
    assert_eq!(node_text(&frame, a), "A");
    assert_eq!(node_text(&frame, b), "B");
}

#[test]
fn mounts_nested_elements() {
    let mut frame = FrameArena::new_in(pool_ref());
    let globals = GlobalArena::new_in(pool_ref());
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
    let mut frame = FrameArena::new_in(pool_ref());
    let globals = GlobalArena::new_in(pool_ref());
    let app = AppContext::from_globals(&globals);

    let root = frame
        .mount(div().flex().flex_col().w_full().p(px(8)), app)
        .unwrap();

    match frame.style(root) {
        Some(style) => {
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
    let mut frame = FrameArena::new_in(pool_ref());
    let globals = GlobalArena::new_in(pool_ref());
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
    let mut frame = FrameArena::new_in(pool_ref());
    let globals = GlobalArena::new_in(pool_ref());
    let app = AppContext::from_globals(&globals);

    let button = frame.mount(div().id("button").child("Press"), app).unwrap();

    assert_eq!(frame.element_id(button), Some(ElementId::Name("button")));

    let text = frame.node(button).first_child.unwrap();

    assert_eq!(node_text(&frame, text), "Press");
}

/// a pool with room for a few nodes and little else
const SMALL_POOL: usize = 4 * 1024;

#[test]
fn reports_out_of_memory_when_the_pool_has_no_room_for_nodes() {
    let mut frame = FrameArena::new_in(pool_ref_of(SMALL_POOL));
    let globals = GlobalArena::new_in(pool_ref());
    let app = AppContext::from_globals(&globals);

    let result = frame.mount(div().children((0..64).map(|_| div())), app);

    assert!(matches!(
        result,
        Err(MountError::OutOfMemory(OutOfMemory {
            table: Table::Nodes,
            ..
        }))
    ));
}

#[test]
fn reports_out_of_memory_when_the_pool_has_no_room_for_text() {
    let mut frame = FrameArena::new_in(pool_ref_of(SMALL_POOL));
    let globals = GlobalArena::new_in(pool_ref());
    let app = AppContext::from_globals(&globals);
    let long = "x".repeat(SMALL_POOL);

    let result = frame.mount(div().child(long.as_str()), app);

    assert_eq!(
        result,
        Err(MountError::OutOfMemory(OutOfMemory {
            table: Table::Text,
            requested: SMALL_POOL,
        }))
    );
}

#[test]
fn node_growth_failure_drops_the_text_it_added() {
    let pool = pool_ref();
    let mut frame = FrameArena::new_in(pool);
    let mut nodes = 0;

    // fill the nodes' first allocation, so the next node has to grow them
    while frame.node_count() < frame.node_capacity() || nodes == 0 {
        frame.push_div(Style::default()).unwrap();
        nodes += 1;
    }
    // room for the text and its style, but not for more nodes
    fill_leaving(pool, 512);

    assert!(matches!(
        frame.push_text("rollback", TextStyle::default()),
        Err(MountError::OutOfMemory(OutOfMemory {
            table: Table::Nodes,
            ..
        }))
    ));
    assert_eq!(frame.text_bytes_used(), 0);
    assert_eq!(frame.node_count(), nodes);
}

#[test]
fn only_identified_or_interactive_nodes_take_extras() {
    let mut frame = FrameArena::new_in(pool_ref());
    let globals = GlobalArena::new_in(pool_ref());
    let app = AppContext::from_globals(&globals);

    let root = frame
        .mount(
            div()
                .child("plain text")
                .child(div())
                .child(div().id("named"))
                .child(div().id("button").focusable()),
            app,
        )
        .unwrap();

    assert_eq!(frame.extras_count(), 2);
    assert_eq!(frame.element_id(root), None);
    assert!(!frame.interaction(root).focusable);

    let button = find_element(&frame, ElementId::Name("button")).unwrap();
    assert!(frame.interaction(button).focusable);
}

#[test]
fn only_nodes_with_interaction_styles_take_table_entries() {
    let mut frame = FrameArena::new_in(pool_ref());
    let globals = GlobalArena::new_in(pool_ref());
    let app = AppContext::from_globals(&globals);

    frame
        .mount(
            div()
                .child(div().id("plain").focusable())
                .child(
                    div()
                        .id("focused")
                        .when_focused(|style| style.bg(Color::RED)),
                )
                .child(
                    div()
                        .id("both")
                        .when_focused(|style| style.bg(Color::RED))
                        .when_pressed(|style| style.bg(Color::GREEN)),
                ),
            app,
        )
        .unwrap();

    // one entry per styled node, holding both its focused and pressed styles
    assert_eq!(frame.interaction_styles.len(), 2);

    let plain = find_element(&frame, ElementId::Name("plain")).unwrap();
    assert_eq!(frame.interaction(plain).styles, None);
}

#[test]
fn clear_resets_frame_storage() {
    let mut frame = FrameArena::new_in(pool_ref());
    let globals = GlobalArena::new_in(pool_ref());
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
    fn render(&mut self, _cx: &mut Context<'_, Self>) -> impl IntoElement {
        div().child("Child")
    }
}

struct Parent {
    child: Entity<Child>,
}

impl Render for Parent {
    fn render(&mut self, _cx: &mut Context<'_, Self>) -> impl IntoElement {
        div().child("Parent").child(self.child)
    }
}

#[test]
fn mounts_entities_as_placeholders() {
    let mut frame = FrameArena::new_in(pool_ref());
    let globals = GlobalArena::new_in(pool_ref());
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
    let entities = EntityArena::new_in(pool_ref());
    let mut frame = FrameArena::new_in(pool_ref());
    let callbacks = CallbackArena::new_in(pool_ref());
    let globals = GlobalArena::new_in(pool_ref());
    let app = AppContext::from_globals(&globals);
    let all_dirty = Cell::new(false);

    let child = entities.insert(Child).unwrap();
    let parent = entities.insert(Parent { child }).unwrap();
    let root = frame.mount(parent, app).unwrap();

    frame
        .expand_entities(RuntimeCx::new(&entities, &globals, &callbacks, &all_dirty))
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
    let entities = EntityArena::new_in(pool_ref());
    let callbacks = CallbackArena::new_in(pool_ref());
    let globals = GlobalArena::new_in(pool_ref());
    let app = AppContext::from_globals(&globals);
    let mut frame = FrameArena::new_in(pool_ref());
    let all_dirty = Cell::new(false);

    let child = entities.insert(Child).unwrap();
    let parent = entities.insert(Parent { child }).unwrap();

    frame.mount(parent, app).unwrap();
    frame
        .expand_entities(RuntimeCx::new(&entities, &globals, &callbacks, &all_dirty))
        .unwrap();

    // if rendering leaked the exclusive borrow, either of these would return BorrowConflict.
    entities.update(parent, |_parent| {}).unwrap();
    entities.update(child, |_child| {}).unwrap();
}

struct Label {
    text: String,
}

impl Render for Label {
    fn render(&mut self, _cx: &mut Context<'_, Self>) -> impl IntoElement {
        div().child(self.text.as_str())
    }
}

#[test]
fn copies_entity_text_into_frame_storage() {
    let entities = EntityArena::new_in(pool_ref());
    let callbacks = CallbackArena::new_in(pool_ref());
    let globals = GlobalArena::new_in(pool_ref());
    let app = AppContext::from_globals(&globals);
    let mut frame = FrameArena::new_in(pool_ref());
    let all_dirty = Cell::new(false);

    let label = entities
        .insert(Label {
            text: String::from("Persistent state text"),
        })
        .unwrap();

    let root = frame.mount(label, app).unwrap();
    frame
        .expand_entities(RuntimeCx::new(&entities, &globals, &callbacks, &all_dirty))
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
    fn render(&mut self, _cx: &mut Context<'_, Self>) -> impl IntoElement {
        div().child(self.child).child(self.child)
    }
}

#[test]
fn rejects_duplicate_entity_mounts() {
    let entities = EntityArena::new_in(pool_ref());
    let callbacks = CallbackArena::new_in(pool_ref());
    let globals = GlobalArena::new_in(pool_ref());
    let app = AppContext::from_globals(&globals);
    let mut frame = FrameArena::new_in(pool_ref());
    let all_dirty = Cell::new(false);

    let child = entities.insert(Child).unwrap();
    let parent = entities.insert(DuplicateParent { child }).unwrap();

    frame.mount(parent, app).unwrap();

    let result = frame.expand_entities(RuntimeCx::new(&entities, &globals, &callbacks, &all_dirty));

    assert_eq!(
        result,
        Err(MountError::DuplicateEntityMount(child.entity_id()))
    );
}

#[test]
fn indexes_mounted_entity_nodes() {
    let entities = EntityArena::new_in(pool_ref());
    let callbacks = CallbackArena::new_in(pool_ref());
    let globals = GlobalArena::new_in(pool_ref());
    let app = AppContext::from_globals(&globals);
    let mut frame = FrameArena::new_in(pool_ref());
    let all_dirty = Cell::new(false);

    let unmounted = entities.insert(Child).unwrap();
    let child = entities.insert(Child).unwrap();
    let parent = entities.insert(Parent { child }).unwrap();

    let root = frame.mount(parent, app).unwrap();
    frame
        .expand_entities(RuntimeCx::new(&entities, &globals, &callbacks, &all_dirty))
        .unwrap();

    assert_eq!(frame.entity_node(parent.entity_id()), Some(root));
    let child_node = frame.entity_node(child.entity_id()).unwrap();
    assert!(matches!(
        frame.node(child_node).kind,
        NodeKind::Entity { entity, .. } if entity == child.entity_id()
    ));
    assert_eq!(frame.entity_node(unmounted.entity_id()), None);

    frame.clear();

    assert_eq!(frame.entity_node(parent.entity_id()), None);
}

#[test]
fn identities_resolve_in_tree_order_when_children_precede_parents() {
    let entities = EntityArena::new_in(pool_ref());
    let mut states = ElementStateTable::new_in(pool_ref());
    let mut frame = FrameArena::new_in(pool_ref());

    let owner = entities.insert(Child).unwrap();

    // the child is mounted before the parent it is attached to, as when an existing
    // subtree moves under a newly mounted node
    let root = frame
        .push_entity(owner.entity_id(), render_entity::<Child>)
        .unwrap();
    let child = frame.push_div(Style::default()).unwrap();
    frame.identify(child, ElementId::Name("child")).unwrap();
    let parent = frame.push_div(Style::default()).unwrap();
    frame.identify(parent, ElementId::Name("parent")).unwrap();
    frame.append_child(parent, child);
    frame.append_child(root, parent);

    frame.resolve_identities(root, &mut states, 1).unwrap();

    let parent_state = state_id(&frame, parent);
    let child_entry = states.entry(state_id(&frame, child)).unwrap();

    assert_eq!(
        child_entry.key.parent,
        IdentityParent::Element(parent_state)
    );
}

#[test]
fn resolving_one_entity_subtree_leaves_other_entities_untouched() {
    let entities = EntityArena::new_in(pool_ref());
    let mut states = ElementStateTable::new_in(pool_ref());
    let mut frame = FrameArena::new_in(pool_ref());

    let rerendered = entities.insert(Child).unwrap();
    let clean = entities.insert(Child).unwrap();

    let root = frame.push_div(Style::default()).unwrap();
    let rerendered_node = frame
        .push_entity(rerendered.entity_id(), render_entity::<Child>)
        .unwrap();
    let kept = frame.push_div(Style::default()).unwrap();
    frame.identify(kept, ElementId::Name("kept")).unwrap();
    let removed = frame.push_div(Style::default()).unwrap();
    frame.identify(removed, ElementId::Name("removed")).unwrap();
    frame.append_child(rerendered_node, kept);
    frame.append_child(rerendered_node, removed);
    let clean_node = frame
        .push_entity(clean.entity_id(), render_entity::<Child>)
        .unwrap();
    let panel = frame.push_div(Style::default()).unwrap();
    frame.identify(panel, ElementId::Name("panel")).unwrap();
    let item = frame.push_div(Style::default()).unwrap();
    frame.identify(item, ElementId::Name("item")).unwrap();
    frame.append_child(panel, item);
    frame.append_child(clean_node, panel);
    frame.append_child(root, rerendered_node);
    frame.append_child(root, clean_node);

    frame.resolve_identities(root, &mut states, 1).unwrap();
    states.sweep(1);

    let kept_state = state_id(&frame, kept);
    let removed_state = state_id(&frame, removed);
    let panel_state = state_id(&frame, panel);
    let item_state = state_id(&frame, item);

    // nested elements belong to the nearest entity above them
    assert_eq!(states.entry(item_state).unwrap().scope, clean.entity_id());

    // the re-rendered entity no longer shows "removed"
    frame.extras_mut(removed).unwrap().element_id = None;
    frame
        .resolve_identities(rerendered_node, &mut states, 2)
        .unwrap();
    states.sweep_scopes(2, |entity| entity == rerendered.entity_id());

    assert_eq!(state_id(&frame, kept), kept_state);
    assert!(!states.contains(removed_state));
    for state in [panel_state, item_state] {
        let entry = states.entry(state).unwrap();

        // the clean entity's states were neither resolved nor swept
        assert_eq!(entry.last_seen_frame, 1);
        assert_eq!(entry.scope, clean.entity_id());
    }
}

#[test]
fn expanding_entities_twice_does_not_duplicate_nodes() {
    let entities = EntityArena::new_in(pool_ref());
    let callbacks = CallbackArena::new_in(pool_ref());
    let globals = GlobalArena::new_in(pool_ref());
    let app = AppContext::from_globals(&globals);
    let mut frame = FrameArena::new_in(pool_ref());
    let all_dirty = Cell::new(false);

    let child = entities.insert(Child).unwrap();
    let parent = entities.insert(Parent { child }).unwrap();

    frame.mount(parent, app).unwrap();
    frame
        .expand_entities(RuntimeCx::new(&entities, &globals, &callbacks, &all_dirty))
        .unwrap();

    let first_count = frame.node_count();

    frame
        .expand_entities(RuntimeCx::new(&entities, &globals, &callbacks, &all_dirty))
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
    fn render(&mut self, cx: &mut Context<'_, Self>) -> impl IntoElement {
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
    let entities = EntityArena::new_in(pool_ref());
    let callbacks = CallbackArena::new_in(pool_ref());
    let globals = GlobalArena::new_in(pool_ref());
    let app = AppContext::from_globals(&globals);
    let mut frame = FrameArena::new_in(pool_ref());
    let all_dirty = Cell::new(false);

    let counter = entities.insert(Counter { value: 0 }).unwrap();

    frame.mount(counter, app).unwrap();
    frame
        .expand_entities(RuntimeCx::new(&entities, &globals, &callbacks, &all_dirty))
        .unwrap();

    let button =
        find_element(&frame, ElementId::Name("increment")).expect("increment element missing");

    assert!(frame.interaction(button).focusable);
}

#[test]
fn mounted_listener_updates_owning_entity() {
    let entities = EntityArena::new_in(pool_ref());
    let callbacks = CallbackArena::new_in(pool_ref());
    let globals = GlobalArena::new_in(pool_ref());
    let app = AppContext::from_globals(&globals);
    let mut frame = FrameArena::new_in(pool_ref());
    let all_dirty = Cell::new(false);

    let counter = entities.insert(Counter { value: 0 }).unwrap();

    frame.mount(counter, app).unwrap();
    frame
        .expand_entities(RuntimeCx::new(&entities, &globals, &callbacks, &all_dirty))
        .unwrap();

    let button =
        find_element(&frame, ElementId::Name("increment")).expect("increment element missing");

    let listener_id = frame
        .event_callbacks(button, TypeId::of::<ActivateEvent>())
        .next()
        .expect("activation listener missing");

    let listener = Listener::<ActivateEvent>::from_id(listener_id);
    callbacks
        .invoke_listener(
            listener,
            &ActivateEvent::new(ElementId::Name("test")),
            RuntimeCx::new(&entities, &globals, &callbacks, &all_dirty),
        )
        .unwrap();

    let value = entities.read(counter, |counter| counter.value);

    assert_eq!(value, Ok(1));
    assert!(entities.is_dirty(counter.entity_id()));
}

struct Status {
    value: i32,
}

struct Controller {
    status: Entity<Status>,
}

impl Controller {
    fn update_status(&mut self, _: &ActivateEvent, cx: &mut Context<Self>) {
        self.status.update(cx, |status, cx| {
            status.value += 10;
            cx.notify();
        });
    }
}

impl Render for Controller {
    fn render(&mut self, cx: &mut Context<'_, Self>) -> impl IntoElement {
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
    let entities = EntityArena::new_in(pool_ref());
    let callbacks = CallbackArena::new_in(pool_ref());
    let globals = GlobalArena::new_in(pool_ref());
    let app = AppContext::from_globals(&globals);
    let mut frame = FrameArena::new_in(pool_ref());
    let all_dirty = Cell::new(false);

    let status = entities.insert(Status { value: 0 }).unwrap();
    let controller = entities.insert(Controller { status }).unwrap();

    frame.mount(controller, app).unwrap();
    frame
        .expand_entities(RuntimeCx::new(&entities, &globals, &callbacks, &all_dirty))
        .unwrap();

    let button = find_element(&frame, ElementId::Name("update-status")).unwrap();

    let listener_id = frame
        .event_callbacks(button, core::any::TypeId::of::<ActivateEvent>())
        .next()
        .expect("activation listener missing");
    let listener = Listener::<ActivateEvent>::from_id(listener_id);
    callbacks
        .invoke_listener(
            listener,
            &ActivateEvent::new(ElementId::Name("test")),
            RuntimeCx::new(&entities, &globals, &callbacks, &all_dirty),
        )
        .unwrap();

    assert_eq!(entities.read(status, |status| status.value,), Ok(10));
    // the listener belongs to the controller, but only the status changed
    assert!(entities.is_dirty(status.entity_id()));
    assert!(!entities.is_dirty(controller.entity_id()));
}

#[test]
fn entity_render_access_errors_become_mount_errors() {
    let entities = EntityArena::new_in(pool_ref());
    let callbacks = CallbackArena::new_in(pool_ref());
    let globals = GlobalArena::new_in(pool_ref());
    let app = AppContext::from_globals(&globals);
    let mut frame = FrameArena::new_in(pool_ref());
    let all_dirty = Cell::new(false);

    let child = entities.insert(Child).unwrap();
    let fake = Entity::<Parent>::from_id(child.entity_id());

    frame.mount(fake, app).unwrap();

    let result = frame.expand_entities(RuntimeCx::new(&entities, &globals, &callbacks, &all_dirty));

    assert_eq!(
        result,
        Err(MountError::EntityAccess(EntityAccessError::TypeMismatch))
    );
}

#[test]
fn expanded_entity_root_is_child_of_entity_node() {
    let entities = EntityArena::new_in(pool_ref());
    let callbacks = CallbackArena::new_in(pool_ref());
    let globals = GlobalArena::new_in(pool_ref());
    let app = AppContext::from_globals(&globals);
    let mut frame = FrameArena::new_in(pool_ref());
    let all_dirty = Cell::new(false);

    let child = entities.insert(Child).unwrap();
    let entity_node = frame.mount(child, app).unwrap();

    frame
        .expand_entities(RuntimeCx::new(&entities, &globals, &callbacks, &all_dirty))
        .unwrap();

    let rendered_root = frame.node(entity_node).first_child.unwrap();

    assert_eq!(frame.node(rendered_root).parent, Some(entity_node));
    assert_eq!(frame.node(rendered_root).next_sibling, None);
}

struct StableApp;

impl Render for StableApp {
    fn render(&mut self, _cx: &mut Context<'_, Self>) -> impl IntoElement {
        div().child(div().id("button").child("Press"))
    }
}

#[test]
fn element_identity_is_stable_across_frames() {
    let entities = EntityArena::new_in(pool_ref());
    let callbacks = CallbackArena::new_in(pool_ref());
    let mut states = ElementStateTable::new_in(pool_ref());
    let globals = GlobalArena::new_in(pool_ref());
    let mut frame = FrameArena::new_in(pool_ref());
    let all_dirty = Cell::new(false);

    let app = entities.insert(StableApp).unwrap();

    build_frame(
        &mut frame,
        &mut states,
        app,
        RuntimeCx::new(&entities, &globals, &callbacks, &all_dirty),
        1,
    )
    .unwrap();

    let button = find_element(&frame, ElementId::Name("button")).unwrap();
    let first = state_id(&frame, button);

    build_frame(
        &mut frame,
        &mut states,
        app,
        RuntimeCx::new(&entities, &globals, &callbacks, &all_dirty),
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
    fn render(&mut self, _cx: &mut Context<'_, Self>) -> impl IntoElement {
        // we can't return two unrelated concrete types from a normal if,
        // so keep the same outer type and conditionally insert unnamed
        // wrappers in separate test component types below.
        div().id("panel").child(div().id("button"))
    }
}

struct WrappedApp;

impl Render for WrappedApp {
    fn render(&mut self, _cx: &mut Context<'_, Self>) -> impl IntoElement {
        div()
            .id("panel")
            .child(div().child(div().child(div().id("button"))))
    }
}

#[test]
fn unnamed_wrappers_do_not_affect_identity_path() {
    let entities = EntityArena::new_in(pool_ref());
    let callbacks = CallbackArena::new_in(pool_ref());
    let mut states = ElementStateTable::new_in(pool_ref());
    let globals = GlobalArena::new_in(pool_ref());
    let mut frame = FrameArena::new_in(pool_ref());
    let all_dirty = Cell::new(false);

    let plain = entities.insert(WrapperApp { _wrapped: false }).unwrap();
    let wrapped = entities.insert(WrappedApp).unwrap();

    // these are different entities, so their Ids should NOT be equal.
    // what we're validating here is the parent relationship inside each
    // tree: unnamed wrappers do not become identity parents.

    build_frame(
        &mut frame,
        &mut states,
        plain,
        RuntimeCx::new(&entities, &globals, &callbacks, &all_dirty),
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
        RuntimeCx::new(&entities, &globals, &callbacks, &all_dirty),
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
    fn render(&mut self, _cx: &mut Context<'_, Self>) -> impl IntoElement {
        div().id("panel").child(div().id("button"))
    }
}

#[test]
fn nested_element_uses_nearest_identified_parent() {
    let entities = EntityArena::new_in(pool_ref());
    let callbacks = CallbackArena::new_in(pool_ref());
    let mut states = ElementStateTable::new_in(pool_ref());
    let globals = GlobalArena::new_in(pool_ref());
    let mut frame = FrameArena::new_in(pool_ref());
    let all_dirty = Cell::new(false);

    let app = entities.insert(NestedApp).unwrap();

    build_frame(
        &mut frame,
        &mut states,
        app,
        RuntimeCx::new(&entities, &globals, &callbacks, &all_dirty),
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
    fn render(&mut self, _cx: &mut Context<'_, Self>) -> impl IntoElement {
        div().child(div().id("button")).child(div().id("button"))
    }
}

#[test]
fn duplicate_ids_in_same_scope_are_rejected() {
    let entities = EntityArena::new_in(pool_ref());
    let callbacks = CallbackArena::new_in(pool_ref());
    let mut states = ElementStateTable::new_in(pool_ref());
    let globals = GlobalArena::new_in(pool_ref());
    let cx = AppContext::from_globals(&globals);
    let mut frame = FrameArena::new_in(pool_ref());
    let all_dirty = Cell::new(false);

    let app = entities.insert(DuplicateApp).unwrap();

    let root = frame.mount(app, cx).unwrap();
    frame
        .expand_entities(RuntimeCx::new(&entities, &globals, &callbacks, &all_dirty))
        .unwrap();

    let result = frame.resolve_identities(root, &mut states, 1);

    assert_eq!(
        result,
        Err(IdentityError::DuplicateElementId {
            id: ElementId::Name("button"),
        })
    );
}

struct SeparateScopesApp;

impl Render for SeparateScopesApp {
    fn render(&mut self, _cx: &mut Context<'_, Self>) -> impl IntoElement {
        div()
            .child(div().id("left").child(div().id("button")))
            .child(div().id("right").child(div().id("button")))
    }
}

#[test]
fn same_local_id_is_allowed_under_different_identified_parents() {
    let entities = EntityArena::new_in(pool_ref());
    let callbacks = CallbackArena::new_in(pool_ref());
    let mut states = ElementStateTable::new_in(pool_ref());
    let globals = GlobalArena::new_in(pool_ref());
    let mut frame = FrameArena::new_in(pool_ref());
    let all_dirty = Cell::new(false);

    let app = entities.insert(SeparateScopesApp).unwrap();

    build_frame(
        &mut frame,
        &mut states,
        app,
        RuntimeCx::new(&entities, &globals, &callbacks, &all_dirty),
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
    fn render(&mut self, _cx: &mut Context<'_, Self>) -> impl IntoElement {
        div().id("button").child("Button")
    }
}

struct WidgetsApp {
    left: Entity<Widget>,
    right: Entity<Widget>,
}

impl Render for WidgetsApp {
    fn render(&mut self, _cx: &mut Context<'_, Self>) -> impl IntoElement {
        div().child(self.left).child(self.right)
    }
}

#[test]
fn separate_entities_have_separate_identity_namespaces() {
    let entities = EntityArena::new_in(pool_ref());
    let callbacks = CallbackArena::new_in(pool_ref());
    let mut states = ElementStateTable::new_in(pool_ref());
    let globals = GlobalArena::new_in(pool_ref());
    let mut frame = FrameArena::new_in(pool_ref());
    let all_dirty = Cell::new(false);

    let left = entities.insert(Widget).unwrap();
    let right = entities.insert(Widget).unwrap();
    let app = entities.insert(WidgetsApp { left, right }).unwrap();

    build_frame(
        &mut frame,
        &mut states,
        app,
        RuntimeCx::new(&entities, &globals, &callbacks, &all_dirty),
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
    fn render(&mut self, _cx: &mut Context<'_, Self>) -> impl IntoElement {
        div().id("internal-button").child("Internal")
    }
}

struct ParentA {
    child: Entity<MovableChild>,
}

impl Render for ParentA {
    fn render(&mut self, _cx: &mut Context<'_, Self>) -> impl IntoElement {
        div().id("left").child(self.child)
    }
}

struct ParentB {
    child: Entity<MovableChild>,
}

impl Render for ParentB {
    fn render(&mut self, _cx: &mut Context<'_, Self>) -> impl IntoElement {
        div()
            .id("right")
            .child(div().child(div().child(self.child)))
    }
}

#[test]
fn moving_entity_does_not_change_internal_element_identity() {
    let entities = EntityArena::new_in(pool_ref());
    let callbacks = CallbackArena::new_in(pool_ref());
    let mut states = ElementStateTable::new_in(pool_ref());
    let globals = GlobalArena::new_in(pool_ref());
    let mut frame = FrameArena::new_in(pool_ref());
    let all_dirty = Cell::new(false);

    let child = entities.insert(MovableChild).unwrap();
    let parent_a = entities.insert(ParentA { child }).unwrap();
    let parent_b = entities.insert(ParentB { child }).unwrap();

    build_frame(
        &mut frame,
        &mut states,
        parent_a,
        RuntimeCx::new(&entities, &globals, &callbacks, &all_dirty),
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
        RuntimeCx::new(&entities, &globals, &callbacks, &all_dirty),
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
    fn render(&mut self, _cx: &mut Context<'_, Self>) -> impl IntoElement {
        div().child(div().id("button"))
    }
}

struct WithoutButton;

impl Render for WithoutButton {
    fn render(&mut self, _cx: &mut Context<'_, Self>) -> impl IntoElement {
        div().child("Nothing")
    }
}

#[test]
fn element_state_is_removed_when_element_disappears() {
    let entities = EntityArena::new_in(pool_ref());
    let callbacks = CallbackArena::new_in(pool_ref());
    let mut states = ElementStateTable::new_in(pool_ref());
    let globals = GlobalArena::new_in(pool_ref());
    let mut frame = FrameArena::new_in(pool_ref());
    let all_dirty = Cell::new(false);

    let with_button = entities.insert(WithButton).unwrap();
    let without_button = entities.insert(WithoutButton).unwrap();

    build_frame(
        &mut frame,
        &mut states,
        with_button,
        RuntimeCx::new(&entities, &globals, &callbacks, &all_dirty),
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
        RuntimeCx::new(&entities, &globals, &callbacks, &all_dirty),
        2,
    )
    .unwrap();
    states.sweep(2);

    assert!(!states.contains(old));
}

#[test]
fn reappearing_element_gets_new_generation() {
    let entities = EntityArena::new_in(pool_ref());
    let callbacks = CallbackArena::new_in(pool_ref());
    let mut states = ElementStateTable::new_in(pool_ref());
    let globals = GlobalArena::new_in(pool_ref());
    let mut frame = FrameArena::new_in(pool_ref());
    let all_dirty = Cell::new(false);

    let with_button = entities.insert(WithButton).unwrap();
    let without_button = entities.insert(WithoutButton).unwrap();

    // frame 1: button exists.
    build_frame(
        &mut frame,
        &mut states,
        with_button,
        RuntimeCx::new(&entities, &globals, &callbacks, &all_dirty),
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
        RuntimeCx::new(&entities, &globals, &callbacks, &all_dirty),
        2,
    )
    .unwrap();

    assert!(!states.contains(old));

    // frame 3: button comes back.
    build_frame(
        &mut frame,
        &mut states,
        with_button,
        RuntimeCx::new(&entities, &globals, &callbacks, &all_dirty),
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
    fn render(&mut self, _cx: &mut Context<'_, Self>) -> impl IntoElement {
        div().child(div().id("a")).child(div().id("b"))
    }
}

#[test]
fn identity_resolution_reports_state_capacity_exhaustion() {
    let entities = EntityArena::new_in(pool_ref());
    let callbacks = CallbackArena::new_in(pool_ref());
    let mut states = ElementStateTable::new_in(pool_ref_with_room(0));
    let globals = GlobalArena::new_in(pool_ref());
    let cx = AppContext::from_globals(&globals);
    let mut frame = FrameArena::new_in(pool_ref());
    let all_dirty = Cell::new(false);

    let app = entities.insert(TooManyStatesApp).unwrap();

    let root = frame.mount(app, cx).unwrap();
    frame
        .expand_entities(RuntimeCx::new(&entities, &globals, &callbacks, &all_dirty))
        .unwrap();

    assert!(matches!(
        frame.resolve_identities(root, &mut states, 1),
        Err(IdentityError::OutOfMemory(OutOfMemory {
            table: Table::ElementStates,
            ..
        }))
    ));
}

#[test]
fn plain_string_mounts_without_text_style_overrides() {
    let mut frame = FrameArena::new_in(pool_ref());
    let globals = GlobalArena::new_in(pool_ref());
    let cx = AppContext::from_globals(&globals);

    let node = frame.mount("Hello", cx).unwrap();

    assert_eq!(frame.text_style_patch(node).unwrap(), TextStyle::default());
    assert_eq!(node_text(&frame, node), "Hello");
}

#[test]
fn explicit_text_element_preserves_text_style_overrides() {
    let mut frame = FrameArena::new_in(pool_ref());
    let globals = GlobalArena::new_in(pool_ref());
    let cx = AppContext::from_globals(&globals);

    let node = frame
        .mount(
            text("Hello")
                .font_weight(FontWeight::BOLD)
                .text_color(Color::RED)
                .line_height(px(18)),
            cx,
        )
        .unwrap();

    assert_eq!(node_text(&frame, node), "Hello");
    assert_eq!(
        frame.text_style_patch(node).unwrap().font_weight,
        Some(FontWeight::BOLD)
    );
    assert_eq!(
        frame.text_style_patch(node).unwrap().color,
        Some(Color::RED)
    );
    assert_eq!(
        frame.text_style_patch(node).unwrap().line_height,
        Some(LineHeight::Pixels(px(18)))
    );
}

#[test]
fn text_style_resolves_through_nested_elements() {
    let mut frame = FrameArena::new_in(pool_ref());
    let globals = GlobalArena::new_in(pool_ref());
    let cx = AppContext::from_globals(&globals);

    let root = frame
        .mount(
            div()
                .font_weight(FontWeight::MEDIUM)
                .text_color(Color::WHITE)
                .line_height(px(14))
                .child(
                    div()
                        .text_color(Color::RED)
                        .child(text("Hello").font_weight(FontWeight::BOLD)),
                ),
            cx,
        )
        .unwrap();

    frame.resolve_text_styles(root);

    let inner = frame.node(root).first_child.unwrap();
    let text_node = frame.node(inner).first_child.unwrap();

    assert_eq!(
        frame.resolved_text_style(text_node),
        ResolvedTextStyle {
            font_weight: FontWeight::BOLD,
            color: Color::RED,
            line_height: LineHeight::Pixels(px(14)),
            ..Default::default()
        }
    );
}

fn node_image_source(frame: &FrameArena, node: NodeId) -> ImageSource {
    match frame.node(node).kind {
        NodeKind::Image { image } => frame.image(image).0,
        _ => panic!("expected image node"),
    }
}

#[test]
fn mounts_image_leaf() {
    let source = ImageSource::new(ImageId::new(1), Size::new(px(16), px(12)));

    let mut frame = FrameArena::new_in(pool_ref());
    let globals = GlobalArena::new_in(pool_ref());
    let cx = AppContext::from_globals(&globals);

    let root = frame.mount(div().child(image(source)), cx).unwrap();

    assert_eq!(frame.node_count(), 2);

    let image_node = frame.node(root).first_child.unwrap();

    assert_eq!(node_image_source(&frame, image_node,), source);
    assert_eq!(frame.node(image_node).parent, Some(root));
    assert_eq!(frame.node(image_node).first_child, None);
}
