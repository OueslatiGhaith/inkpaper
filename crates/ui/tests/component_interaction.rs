use std::{cell::Cell, rc::Rc};

use inkpaper_ui::prelude::*;

type TestRuntime = Runtime;

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
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
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
    fn render(&mut self, cx: &mut Context<'_, Self>) -> impl IntoElement {
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

    let mut runtime = TestRuntime::new(pool());

    runtime
        .create_root({
            let first_activations = first_activations.clone();
            let second_activations = second_activations.clone();

            move |_| App {
                first_activations,
                second_activations,
            }
        })
        .unwrap();

    runtime.rebuild().unwrap();

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

    let mut runtime = TestRuntime::new(pool());

    runtime
        .create_root({
            let first_activations = first_activations.clone();
            let second_activations = second_activations.clone();

            move |_| App {
                first_activations,
                second_activations,
            }
        })
        .unwrap();

    runtime.rebuild().unwrap();

    assert!(runtime.focus_next());

    let target = runtime
        .focused_target()
        .expect("first component should have a stable event target");

    runtime.rebuild().unwrap();

    assert!(
        runtime
            .dispatch_to(target, &ActivateEvent::new(ElementId::Name("first")))
            .unwrap()
    );

    assert_eq!(first_activations.get(), 1);
    assert_eq!(second_activations.get(), 0);
}

#[test]
fn hit_at_returns_laid_out_bounds_and_target_for_custom_events() {
    #[derive(Clone, Copy)]
    struct ScrubEvent(i32);

    struct TrackApp {
        last_value: Rc<Cell<i32>>,
    }

    impl TrackApp {
        fn on_scrub(&mut self, event: &ScrubEvent, _: &mut Context<'_, Self>) {
            self.last_value.set(event.0);
        }
    }

    impl Render for TrackApp {
        fn render(&mut self, cx: &mut Context<'_, Self>) -> impl IntoElement {
            let on_scrub = cx.listener(Self::on_scrub);

            div().w(px(200)).h(px(100)).relative().child(
                div()
                    .id("slider-track")
                    .on(on_scrub)
                    .absolute()
                    .left(px(40))
                    .top(px(20))
                    .w(px(100))
                    .h(px(30)),
            )
        }
    }

    let last_value = Rc::new(Cell::new(-1));
    let mut runtime = Runtime::<inkpaper_ui::RuntimeResources<'_, 0, 1, 1, 0>>::new(pool());
    runtime
        .create_root({
            let last_value = last_value.clone();
            move |_| TrackApp { last_value }
        })
        .unwrap();

    runtime.prepare_frame(Size::new(px(200), px(100))).unwrap();

    // Outside the track misses.
    assert_eq!(runtime.hit_at::<ScrubEvent>(Point::new(px(10), px(10))), None);

    // Inside the track returns its id and laid-out screen bounds.
    let hit = runtime
        .hit_at::<ScrubEvent>(Point::new(px(65), px(35)))
        .expect("point inside track should hit");
    assert_eq!(hit.id(), ElementId::Name("slider-track"));
    assert_eq!(
        hit.bounds(),
        Rect::new(Point::new(px(40), px(20)), Size::new(px(100), px(30))),
    );

    // Subsequent events dispatch directly to the captured target.
    assert!(runtime.dispatch_to(hit.target(), &ScrubEvent(77)).unwrap());
    assert_eq!(last_value.get(), 77);
}

/// a pool large enough for this file's runtimes, over leaked memory
fn pool() -> inkpaper_ui::Pool {
    inkpaper_ui::Pool::new(Box::leak(Box::<[u8]>::new_uninit_slice(256 * 1024)))
}
