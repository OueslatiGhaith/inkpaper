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

struct ConditionalChildren {
    show_footer: bool,
    highlighted: bool,
}

impl Render for ConditionalChildren {
    fn render<'a>(&'a mut self, _: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        div()
            .when(self.highlighted, |root| root.bg(Color::rgb(40, 50, 70)))
            .w_full()
            .child("Always")
            .when(self.show_footer, |root| root.child("Footer"))
    }
}

#[test]
fn when_can_change_the_concrete_builder_type() {
    let mut without_footer = TestRuntime::default();

    without_footer
        .create_root(|_| ConditionalChildren {
            show_footer: false,
            highlighted: false,
        })
        .unwrap();

    without_footer.rebuild().unwrap();

    let nodes_without_footer = without_footer.frame_node_count();
    let text_without_footer = without_footer.frame_text_bytes_used();

    let mut with_footer = TestRuntime::default();

    with_footer
        .create_root(|_| ConditionalChildren {
            show_footer: true,
            highlighted: true,
        })
        .unwrap();

    with_footer.rebuild().unwrap();

    assert_eq!(with_footer.frame_node_count(), nodes_without_footer + 1);
    assert_eq!(
        with_footer.frame_text_bytes_used(),
        text_without_footer + "Footer".len()
    );
}

#[test]
fn when_does_not_run_transform_when_condition_is_false() {
    let calls = Cell::new(0);

    let _element = div().when(false, |root| {
        calls.set(calls.get() + 1);
        root.bg(Color::BLUE)
    });

    assert_eq!(calls.get(), 0);
}

#[test]
fn when_runs_transform_once_when_condition_is_true() {
    let calls = Cell::new(0);

    let _element = div().when(true, |root| {
        calls.set(calls.get() + 1);
        root.bg(Color::BLUE)
    });

    assert_eq!(calls.get(), 1);
}

struct OptionalContent {
    message: Option<&'static str>,
}

impl Render for OptionalContent {
    fn render<'a>(&'a mut self, _: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        div()
            .child("Status")
            .when_some(self.message, |root, message| root.child(message))
    }
}

#[test]
fn when_some_adds_content_only_for_some_values() {
    let mut without_message = TestRuntime::default();

    without_message
        .create_root(|_| OptionalContent { message: None })
        .unwrap();

    without_message.rebuild().unwrap();

    let nodes_without_message = without_message.frame_node_count();
    let text_without_message = without_message.frame_text_bytes_used();

    let mut with_message = TestRuntime::default();

    with_message
        .create_root(|_| OptionalContent {
            message: Some("Ready"),
        })
        .unwrap();

    with_message.rebuild().unwrap();

    assert_eq!(with_message.frame_node_count(), nodes_without_message + 1);
    assert_eq!(
        with_message.frame_text_bytes_used(),
        text_without_message + "Ready".len()
    );
}

#[test]
fn when_some_does_not_run_transform_for_none() {
    let calls = Cell::new(0);

    let _element = div().when_some(None::<u32>, |root, value| {
        calls.set(calls.get() + value);
        root
    });

    assert_eq!(calls.get(), 0);
}

struct InteractiveConditional {
    activations: Rc<Cell<u32>>,
    emphasized: bool,
}

impl InteractiveConditional {
    fn activated(&mut self, _: &ActivateEvent, _: &mut Context<'_, Self>) {
        self.activations
            .set(self.activations.get().saturating_add(1));
    }
}

impl Render for InteractiveConditional {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        let activated = cx.listener(Self::activated);

        div().child(
            div()
                .w(px(60))
                .h(px(20))
                .bg(Color::rgb(40, 60, 90))
                .when(self.emphasized, |button| {
                    button.border(px(1)).border_color(Color::WHITE)
                })
                .id("button")
                .when(self.emphasized, |button| {
                    button.when_focused(|style| style.border_color(Color::WHITE))
                })
                .on_activate(activated),
        )
    }
}

#[test]
fn conditional_composition_preserves_interaction_capabilities() {
    let activations = Rc::new(Cell::new(0));
    let mut runtime = TestRuntime::default();

    runtime
        .create_root({
            let activations = activations.clone();

            move |_| InteractiveConditional {
                activations,
                emphasized: true,
            }
        })
        .unwrap();

    runtime.rebuild().unwrap();

    assert!(runtime.focus_next());
    runtime.take_invalidation();

    assert!(runtime.begin_focused_activation());
    runtime.take_invalidation();

    assert!(runtime.complete_focused_activation().unwrap());
    assert_eq!(activations.get(), 1);
}
