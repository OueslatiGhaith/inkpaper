use std::{format, rc::Rc, string::String, vec::Vec};

use crate::{frame::NodeKind, *};

type TestRuntime = Runtime<TestStorage<8192, 16, 8192, 128, 256, 1024, 32, 64, 2>>;

struct Label {
    text: &'static str,
    renders: u32,
    activations: u32,
    /// shared with every listener this label registered, to count the live ones
    capture: Rc<()>,
}

impl Label {
    fn new(text: &'static str) -> Self {
        Self {
            text,
            renders: 0,
            activations: 0,
            capture: Rc::new(()),
        }
    }
}

impl Render for Label {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        self.renders += 1;

        let capture = self.capture.clone();
        let activate = cx.listener(move |label: &mut Label, _: &ActivateEvent, _| {
            let _ = &capture;
            label.activations += 1;
        });

        div().id(self.text).on_activate(activate).child(self.text)
    }
}

struct Shell {
    first: Entity<Label>,
    second: Entity<Label>,
    show_second: bool,
    /// notified by the shell's render
    touch_first: bool,
    renders: u32,
}

impl Render for Shell {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        self.renders += 1;

        if self.touch_first {
            self.first
                .update(cx, |label, cx| {
                    label.text = "touched";
                    cx.notify();
                })
                .unwrap();
        }

        div()
            .id("shell")
            .child(self.first)
            .children(self.show_second.then_some(self.second))
    }
}

struct Fixture {
    runtime: TestRuntime,
    shell: Entity<Shell>,
    first: Entity<Label>,
    second: Entity<Label>,
}

fn fixture(partial: bool) -> Fixture {
    let mut runtime = TestRuntime::default();
    let first = runtime.create(|_| Label::new("first")).unwrap();
    let second = runtime.create(|_| Label::new("second")).unwrap();
    let shell = runtime
        .create_root(|_| Shell {
            first,
            second,
            show_second: true,
            touch_first: false,
            renders: 0,
        })
        .unwrap();

    runtime.set_partial_rebuilds(partial);
    runtime.rebuild().unwrap();
    runtime.take_invalidation();

    Fixture {
        runtime,
        shell,
        first,
        second,
    }
}

impl Fixture {
    fn label(&self, label: Entity<Label>) -> (u32, u32) {
        self.runtime
            .update(label, |label, _| (label.renders, label.activations))
            .unwrap()
    }

    fn shell_renders(&self) -> u32 {
        self.runtime
            .update(self.shell, |shell, _| shell.renders)
            .unwrap()
    }

    fn notify<T: 'static>(&self, entity: Entity<T>) {
        self.runtime.update(entity, |_, cx| cx.notify()).unwrap();
    }

    /// takes the invalidation and rebuilds, as the presenters do
    fn present(&mut self) {
        assert_eq!(self.runtime.take_invalidation(), Invalidation::Rebuild);
        self.runtime.rebuild().unwrap();
    }

    /// activates the `index`th focusable element
    fn activate(&mut self, index: usize) -> bool {
        self.runtime.clear_focus();
        for _ in 0..=index {
            assert!(self.runtime.focus_next());
        }

        self.runtime.activate_focused().unwrap()
    }

    /// the tree under the root, in tree order
    fn outline(&self) -> Vec<String> {
        let frame = self.runtime.frame();
        let mut outline = Vec::new();
        let mut current = self.runtime.root_node();

        while let Some(node) = current {
            let depth = core::iter::successors(frame.node(node).parent, |&parent| {
                frame.node(parent).parent
            })
            .count();
            let kind = match frame.node(node).kind {
                NodeKind::Div { .. } => String::from("div"),
                NodeKind::Text { text } => format!("{:?}", frame.text(text)),
                NodeKind::Entity { entity, .. } => format!("entity {}", entity.slot()),
                _ => String::from("other"),
            };

            outline.push(format!("{depth} {kind}"));
            current = frame.next_depth_first_node(node);
        }

        outline
    }
}

#[test]
fn partial_rebuild_renders_only_the_notified_entity() {
    let mut fixture = fixture(true);

    fixture.notify(fixture.second);
    fixture.present();

    assert_eq!(fixture.shell_renders(), 1);
    assert_eq!(fixture.label(fixture.first), (1, 0));
    assert_eq!(fixture.label(fixture.second), (2, 0));

    // a parent renders again without its clean children
    fixture.notify(fixture.shell);
    fixture.present();

    assert_eq!(fixture.shell_renders(), 2);
    assert_eq!(fixture.label(fixture.first), (1, 0));
    assert_eq!(fixture.label(fixture.second), (2, 0));
}

#[test]
fn partial_rebuilds_are_off_by_default() {
    let mut fixture = fixture(false);

    fixture.notify(fixture.second);
    fixture.present();

    assert_eq!(fixture.shell_renders(), 2);
    assert_eq!(fixture.label(fixture.first), (2, 0));
    assert_eq!(fixture.label(fixture.second), (2, 0));
}

#[test]
fn partial_rebuild_matches_a_full_rebuild() {
    let mut partial = fixture(true);
    let mut full = fixture(false);

    for fixture in [&mut partial, &mut full] {
        fixture
            .runtime
            .update(fixture.second, |label, cx| {
                label.text = "changed";
                cx.notify();
            })
            .unwrap();
        fixture.present();
    }

    assert_eq!(partial.outline(), full.outline());

    for fixture in [&mut partial, &mut full] {
        fixture
            .runtime
            .update(fixture.shell, |shell, cx| {
                shell.show_second = false;
                cx.notify();
            })
            .unwrap();
        fixture.present();
    }

    assert_eq!(partial.outline(), full.outline());
    assert!(partial.runtime.frame().detached_node_count() > 0);
}

#[test]
fn listeners_of_clean_and_rerendered_entities_stay_callable() {
    let mut fixture = fixture(true);
    let first_capture = fixture
        .runtime
        .update(fixture.first, |label, _| label.capture.clone())
        .unwrap();

    fixture.notify(fixture.first);
    fixture.present();

    // the previous render's listener was released, and its capture dropped
    assert_eq!(Rc::strong_count(&first_capture), 3);

    assert!(fixture.activate(0));
    assert!(fixture.activate(1));
    assert_eq!(fixture.label(fixture.first), (2, 1));
    assert_eq!(fixture.label(fixture.second), (1, 1));
}

#[test]
fn entities_that_leave_the_frame_release_their_listeners() {
    let mut fixture = fixture(true);
    let second_capture = fixture
        .runtime
        .update(fixture.second, |label, _| label.capture.clone())
        .unwrap();

    fixture
        .runtime
        .update(fixture.shell, |shell, cx| {
            shell.show_second = false;
            cx.notify();
        })
        .unwrap();
    fixture.present();

    // only the label and this test hold the capture
    assert_eq!(Rc::strong_count(&second_capture), 2);
    assert_eq!(
        fixture
            .runtime
            .frame()
            .entity_node(fixture.second.entity_id()),
        None
    );

    // mounted again, it renders from scratch
    fixture
        .runtime
        .update(fixture.shell, |shell, cx| {
            shell.show_second = true;
            cx.notify();
        })
        .unwrap();
    fixture.present();

    assert_eq!(fixture.label(fixture.second), (2, 0));
    assert!(fixture.activate(1));
    assert_eq!(fixture.label(fixture.second), (2, 1));
}

#[test]
fn focus_in_a_clean_entity_survives_a_partial_rebuild() {
    let mut fixture = fixture(true);

    fixture.runtime.clear_focus();
    assert!(fixture.runtime.focus_next());
    assert!(fixture.runtime.focus_next());
    let focused = fixture.runtime.focused_target().unwrap();

    fixture.notify(fixture.first);
    fixture.present();

    assert_eq!(fixture.runtime.focused_target(), Some(focused));
    assert!(fixture.runtime.activate_focused().unwrap());
    assert_eq!(fixture.label(fixture.second), (1, 1));
}

#[test]
fn a_child_notified_by_its_parents_render_renders_again() {
    let mut fixture = fixture(true);

    fixture
        .runtime
        .update(fixture.shell, |shell, cx| {
            shell.touch_first = true;
            cx.notify();
        })
        .unwrap();
    fixture.present();

    assert_eq!(fixture.label(fixture.first), (2, 0));
    assert!(
        fixture
            .outline()
            .iter()
            .any(|line| line.ends_with("\"touched\""))
    );
    // the child rendered in this rebuild, so it isn't dirty again
    assert_eq!(fixture.runtime.invalidation(), Invalidation::None);
}

#[test]
fn global_changes_rebuild_fully() {
    struct Theme;

    impl Global for Theme {}

    let mut fixture = fixture(true);

    fixture.runtime.set_global(Theme).unwrap();
    fixture.present();

    assert_eq!(fixture.shell_renders(), 2);
    assert_eq!(fixture.label(fixture.first), (2, 0));
    assert_eq!(fixture.runtime.frame().detached_node_count(), 0);
}

#[test]
fn many_detached_nodes_lead_to_a_full_rebuild() {
    let mut fixture = fixture(true);

    // each shell render detaches its div and two entity nodes. Once more than half the
    // frame is detached, the next rebuild is full and renders the labels too
    for _ in 0..4 {
        fixture.notify(fixture.shell);
        fixture.present();
    }

    assert_eq!(fixture.label(fixture.first).0, 2);
    assert_eq!(fixture.runtime.frame().detached_node_count(), 0);
}
