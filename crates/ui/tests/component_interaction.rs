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

struct Button<'a> {
    label: &'a str,
    style: Style,
}

impl<'a> Button<'a> {
    fn new(label: &'a str) -> Self {
        Self {
            label,
            style: Style::default(),
        }
        .flex()
        .items_center()
        .justify_center()
        .px(px(8))
        .py(px(4))
        .bg(Color::rgb(45, 65, 90))
        .rounded(px(4))
    }
}

impl Styled for Button<'_> {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

impl RenderOnce for Button<'_> {
    fn render(self) -> impl IntoElement {
        let mut root = div();
        *root.style_mut() = self.style;

        root.child(self.label)
    }
}

struct App {
    first_activations: Rc<Cell<u32>>,
    second_activations: Rc<Cell<u32>>,
}

impl App {
    fn first_activated(&mut self, _: &ActivateEvent, _: &mut Context<'_, Self>) {
        self.first_activations
            .set(self.first_activations.get().saturating_add(1));
    }

    fn second_activated(&mut self, _: &ActivateEvent, _: &mut Context<'_, Self>) {
        self.second_activations
            .set(self.second_activations.get().saturating_add(1));
    }
}

impl Render for App {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        let first_activated = cx.listener(Self::first_activated);
        let second_activated = cx.listener(Self::second_activated);

        div()
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .gap(px(4))
            .child(
                Button::new("First")
                    .id("first")
                    .on_activate(first_activated)
                    .when_focused(|style| style.border_color(Color::WHITE))
                    .when_pressed(|style| style.bg(Color::rgb(70, 100, 140))),
            )
            .child(
                Button::new("Second")
                    .id("second")
                    .on_activate(second_activated)
                    .when_focused(|style| style.border_color(Color::WHITE))
                    .when_pressed(|style| style.bg(Color::rgb(70, 100, 140))),
            )
    }
}

#[test]
fn render_once_component_can_receive_external_identity_and_activation() {
    let first_activations = Rc::new(Cell::new(0));
    let second_activations = Rc::new(Cell::new(0));

    let mut runtime = TestRuntime::default();

    let app = runtime
        .create({
            let first_activations = first_activations.clone();
            let second_activations = second_activations.clone();

            move |_| App {
                first_activations,
                second_activations,
            }
        })
        .unwrap();

    runtime.rebuild(app).unwrap();

    assert!(runtime.focus_next());
    assert_eq!(runtime.take_invalidation(), Invalidation::Paint);

    assert!(runtime.begin_focused_activation());
    assert_eq!(runtime.take_invalidation(), Invalidation::Paint);

    assert!(runtime.complete_focused_activation().unwrap());
    assert_eq!(first_activations.get(), 1);
    assert_eq!(second_activations.get(), 0);
    assert_eq!(runtime.take_invalidation(), Invalidation::Paint);

    assert!(runtime.focus_next());
    assert_eq!(runtime.take_invalidation(), Invalidation::Paint);

    assert!(runtime.begin_focused_activation());
    assert_eq!(runtime.take_invalidation(), Invalidation::Paint);

    assert!(runtime.complete_focused_activation().unwrap());
    assert_eq!(first_activations.get(), 1);
    assert_eq!(second_activations.get(), 1);
    assert_eq!(runtime.take_invalidation(), Invalidation::Paint);
}

#[test]
fn component_event_target_survives_rebuild() {
    let first_activations = Rc::new(Cell::new(0));
    let second_activations = Rc::new(Cell::new(0));

    let mut runtime = TestRuntime::default();

    let app = runtime
        .create({
            let first_activations = first_activations.clone();
            let second_activations = second_activations.clone();

            move |_| App {
                first_activations,
                second_activations,
            }
        })
        .unwrap();

    runtime.rebuild(app).unwrap();

    assert!(runtime.focus_next());

    let target = runtime
        .focused_target()
        .expect("first component should have a stable event target");

    runtime.rebuild(app).unwrap();

    assert!(runtime.dispatch_to(target, &ActivateEvent).unwrap());

    assert_eq!(first_activations.get(), 1);
    assert_eq!(second_activations.get(), 0);
}
