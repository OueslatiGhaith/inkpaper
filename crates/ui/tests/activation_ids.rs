use std::{cell::RefCell, rc::Rc};

use inkpaper_ui::{ResolvedTextStyle, TextMeasurer, prelude::*};

type TestRuntime = Runtime<
    4096, // entity bytes
    1,    // entity slots
    4096, // callback bytes
    16,   // callback slots
    64,   // frame nodes
    1024, // frame text bytes
    32,   // element states
>;

// the rows have no text
struct NoText;

impl TextMeasurer for NoText {
    fn measure_text(&self, _: &str, _: ResolvedTextStyle, _: Size) -> Size {
        Size::new(px(0), px(0))
    }
}

/// Three rows sharing one listener.
struct SharedListenerList {
    activated: Rc<RefCell<Vec<Option<usize>>>>,
}

impl SharedListenerList {
    fn activated(&mut self, event: &ActivateEvent, _: &mut Context<'_, Self>) {
        self.activated.borrow_mut().push(event.index());
    }
}

impl Render for SharedListenerList {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        let listener = cx.listener(Self::activated);

        div()
            .w(px(100))
            .flex()
            .flex_col()
            .children((0..3usize).map(move |index| {
                div()
                    .id(("row", index))
                    .on_activate(listener)
                    .w_full()
                    .h(px(20))
            }))
    }
}

#[test]
fn a_shared_listener_learns_which_element_was_activated() {
    let activated = Rc::new(RefCell::new(Vec::new()));

    let mut runtime = TestRuntime::default();

    runtime
        .create_root({
            let activated = activated.clone();

            move |_| SharedListenerList { activated }
        })
        .unwrap();

    runtime.rebuild().unwrap();
    runtime
        .layout_with_measurer(Size::new(px(100), px(100)), &NoText)
        .unwrap();

    for y in [50, 10] {
        let point = Point::new(px(10), px(y));

        assert!(runtime.begin_activation_at(point));
        assert!(runtime.complete_activation_at(point).unwrap());
    }

    assert_eq!(*activated.borrow(), [Some(2), Some(0)]);
}
