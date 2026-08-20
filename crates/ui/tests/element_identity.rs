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

fn draw_nothing(_: Rect, _: &mut dyn CanvasPainter) {}

struct Badge<'a> {
    label: &'a str,
}

impl RenderOnce for Badge<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        div().p(px(2)).child(self.label)
    }
}

struct IdentityKinds {
    conditional_content: bool,
}

impl Render for IdentityKinds {
    fn render<'a>(&'a mut self, _: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        let source = ImageSource::new(ImageId::new(0), Size::new(px(8), px(8)));

        div()
            .child(text("Text").id("text").focusable())
            .child(image(source).id("image").focusable())
            .child(canvas(draw_nothing).id("canvas").focusable())
            .child(Badge { label: "Badge" }.id("component").focusable())
            .child(
                div()
                    .when(self.conditional_content, |root| root.child("Conditional"))
                    .id("conditional")
                    .focusable(),
            )
    }
}

#[test]
fn every_element_kind_can_receive_identity() {
    let mut runtime = TestRuntime::default();

    let app = runtime
        .create(|_| IdentityKinds {
            conditional_content: true,
        })
        .unwrap();

    runtime.rebuild(app).unwrap();

    let mut targets = [None; 5];

    for target in &mut targets {
        assert!(runtime.focus_next());
        *target = runtime.focused_target();
        assert!(target.is_some());
    }

    for left in 0..targets.len() {
        for right in left + 1..targets.len() {
            assert_ne!(targets[left], targets[right]);
        }
    }
}

struct ReidentifiedApp {
    activations: Rc<Cell<u32>>,
}

impl ReidentifiedApp {
    fn activated(&mut self, _: &ActivateEvent, _: &mut Context<'_, Self>) {
        self.activations
            .set(self.activations.get().saturating_add(1));
    }
}

impl Render for ReidentifiedApp {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        let activated = cx.listener(Self::activated);

        div().child(
            div()
                .w(px(40))
                .h(px(20))
                .id("original")
                .on_activate(activated)
                .id("replacement"),
        )
    }
}

#[test]
fn reidentifying_wrapped_element_preserves_interactivity() {
    let activations = Rc::new(Cell::new(0));

    let mut runtime = TestRuntime::default();

    let app = runtime
        .create({
            let activations = activations.clone();

            move |_| ReidentifiedApp { activations }
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

#[test]
fn reidentifying_stateful_element_replaces_id_without_losing_state() {
    let element = div().id("first").focusable().id("second");

    assert_eq!(element.element_id(), inkpaper_ui::ElementId::Name("second"),);
}
