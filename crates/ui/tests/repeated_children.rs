use std::{cell::Cell, rc::Rc};

use inkpaper_ui::prelude::*;

type TestRuntime = Runtime<
    4096, // entity bytes
    1,    // entity slots
    4096, // callback bytes
    16,   // callback slots
    64,   // frame nodes
    1024, // frame text bytes
    32,   // element states
>;

struct EmptyChildren;

impl Render for EmptyChildren {
    fn render<'a>(&'a mut self, _: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        div()
    }
}

struct ArrayChildren;

impl Render for ArrayChildren {
    fn render<'a>(&'a mut self, _: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        div().children(["One", "Two", "Three"])
    }
}

#[test]
fn children_mounts_every_item_from_an_array() {
    let mut empty_runtime = TestRuntime::default();

    let empty = empty_runtime.create(|_| EmptyChildren).unwrap();

    empty_runtime.rebuild(empty).unwrap();

    let empty_nodes = empty_runtime.frame_node_count();
    let empty_text = empty_runtime.frame_text_bytes_used();

    let mut list_runtime = TestRuntime::default();

    let list = list_runtime.create(|_| ArrayChildren).unwrap();

    list_runtime.rebuild(list).unwrap();

    assert_eq!(list_runtime.frame_node_count(), empty_nodes + 3,);

    assert_eq!(
        list_runtime.frame_text_bytes_used(),
        empty_text + "One".len() + "Two".len() + "Three".len(),
    );
}

struct ListRow<'a> {
    label: &'a str,
}

impl RenderOnce for ListRow<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        div().h(px(12)).child(self.label)
    }
}

struct BorrowedIterator {
    labels: [&'static str; 3],
}

impl Render for BorrowedIterator {
    fn render<'a>(&'a mut self, _: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        div()
            .flex()
            .flex_col()
            .children(self.labels.iter().copied().map(|label| ListRow { label }))
    }
}

#[test]
fn children_accepts_borrowed_iterators_of_render_once_components() {
    let mut runtime = TestRuntime::default();

    let app = runtime
        .create(|_| BorrowedIterator {
            labels: ["Wi-Fi", "Bluetooth", "Display"],
        })
        .unwrap();

    runtime.rebuild(app).unwrap();

    assert_eq!(
        runtime.frame_text_bytes_used(),
        "Wi-Fi".len() + "Bluetooth".len() + "Display".len(),
    );

    assert!(runtime.frame_node_count() >= 8);
}

struct EmptyIterator;

impl Render for EmptyIterator {
    fn render<'a>(&'a mut self, _: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        div()
            .child("Before")
            .children(core::iter::empty::<&'static str>())
            .child("After")
    }
}

struct WithoutEmptyIterator;

impl Render for WithoutEmptyIterator {
    fn render<'a>(&'a mut self, _: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        div().child("Before").child("After")
    }
}

#[test]
fn empty_children_iterator_adds_no_placeholder_node() {
    let mut with_empty = TestRuntime::default();

    let app = with_empty.create(|_| EmptyIterator).unwrap();

    with_empty.rebuild(app).unwrap();

    let mut without_empty = TestRuntime::default();

    let app = without_empty.create(|_| WithoutEmptyIterator).unwrap();

    without_empty.rebuild(app).unwrap();

    assert_eq!(
        with_empty.frame_node_count(),
        without_empty.frame_node_count(),
    );

    assert_eq!(
        with_empty.frame_text_bytes_used(),
        without_empty.frame_text_bytes_used(),
    );
}

struct InteractiveList {
    activations: Rc<Cell<u32>>,
}

impl InteractiveList {
    fn activated(&mut self, _: &ActivateEvent, _: &mut Context<'_, Self>) {
        self.activations
            .set(self.activations.get().saturating_add(1));
    }
}

impl Render for InteractiveList {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        let activated = cx.listener(Self::activated);

        div().child(
            div()
                .id("list")
                .on_activate(activated)
                .children(["One", "Two", "Three"]),
        )
    }
}

#[test]
fn children_composes_through_identity_and_event_wrappers() {
    let activations = Rc::new(Cell::new(0));

    let mut runtime = TestRuntime::default();

    let app = runtime
        .create({
            let activations = activations.clone();

            move |_| InteractiveList { activations }
        })
        .unwrap();

    runtime.rebuild(app).unwrap();

    assert!(runtime.focus_next());
    runtime.take_invalidation();

    assert!(runtime.begin_focused_activation());
    runtime.take_invalidation();

    assert!(runtime.complete_focused_activation().unwrap());
    assert_eq!(activations.get(), 1);
}

struct ConditionalList {
    highlighted: bool,
}

impl Render for ConditionalList {
    fn render<'a>(&'a mut self, _: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        div()
            .when(self.highlighted, |root| root.bg(Color::rgb(40, 50, 70)))
            .children(["First", "Second", "Third"])
    }
}

#[test]
fn children_composes_through_conditional_branches() {
    let mut runtime = TestRuntime::default();

    let app = runtime
        .create(|_| ConditionalList { highlighted: true })
        .unwrap();

    runtime.rebuild(app).unwrap();

    assert_eq!(
        runtime.frame_text_bytes_used(),
        "First".len() + "Second".len() + "Third".len(),
    );
}
