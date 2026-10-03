use std::{cell::RefCell, rc::Rc};

use inkpaper_ui::{ResolvedTextStyle, TextMeasurer, prelude::*};

type TestRuntime = Runtime<
    FixedStorage<
        4096, // entity bytes
        1,    // entity slots
        4096, // callback bytes
        16,   // callback slots
        64,   // frame nodes
        1024, // frame text bytes
        32,   // element states
    >,
>;

// the rows have no text
struct NoText;

impl TextMeasurer for NoText {
    fn measure_text(&self, _: &str, _: ResolvedTextStyle, _: Size) -> Size {
        Size::new(px(0), px(0))
    }
}

/// An event the framework doesn't know about, defined by the application.
struct Custom {
    id: ElementId,
}

/// Three rows sharing one listener. The last one also takes `Custom` events.
struct SharedListenerList {
    activated: Rc<RefCell<Vec<Option<usize>>>>,
    custom: Rc<RefCell<Vec<ElementId>>>,
}

impl SharedListenerList {
    fn activated(&mut self, event: &ActivateEvent, _: &mut Context<'_, Self>) {
        self.activated.borrow_mut().push(event.index());
    }

    fn custom(&mut self, event: &Custom, _: &mut Context<'_, Self>) {
        self.custom.borrow_mut().push(event.id);
    }
}

impl Render for SharedListenerList {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        let listener = cx.listener(Self::activated);
        let custom = cx.listener(Self::custom);

        div()
            .w(px(100))
            .flex()
            .flex_col()
            .children((0..3usize).map(move |index| {
                div()
                    .id(("row", index))
                    .on_activate(listener)
                    .when(index == 2, |row| row.on::<Custom>(custom))
                    .w_full()
                    .h(px(20))
            }))
    }
}

type Events = (Rc<RefCell<Vec<Option<usize>>>>, Rc<RefCell<Vec<ElementId>>>);

fn laid_out_list() -> (TestRuntime, Events) {
    let activated = Rc::new(RefCell::new(Vec::new()));
    let custom = Rc::new(RefCell::new(Vec::new()));

    let mut runtime = TestRuntime::default();

    runtime
        .create_root({
            let activated = activated.clone();
            let custom = custom.clone();

            move |_| SharedListenerList { activated, custom }
        })
        .unwrap();

    runtime.rebuild().unwrap();
    runtime
        .layout_with_measurer(Size::new(px(100), px(100)), &NoText)
        .unwrap();

    (runtime, (activated, custom))
}

#[test]
fn a_shared_listener_learns_which_element_was_activated() {
    let (mut runtime, (activated, _)) = laid_out_list();

    for y in [50, 10] {
        let point = Point::new(px(10), px(y));

        assert!(runtime.begin_activation_at(point));
        assert!(runtime.complete_activation_at(point).unwrap());
    }

    assert_eq!(*activated.borrow(), [Some(2), Some(0)]);
}

#[test]
fn an_application_event_carries_the_id_of_the_element_listening_for_it() {
    let (mut runtime, (activated, custom)) = laid_out_list();

    let dispatched = |runtime: &mut TestRuntime, y| {
        RuntimeApi::dispatch_at_with(runtime, Point::new(px(10), px(y)), |id| Custom { id })
            .unwrap()
    };

    // the first row doesn't listen for it
    assert!(!dispatched(&mut runtime, 10));
    assert!(dispatched(&mut runtime, 50));

    assert_eq!(*custom.borrow(), [ElementId::NamedValue("row", 2)]);
    assert!(activated.borrow().is_empty());
}
