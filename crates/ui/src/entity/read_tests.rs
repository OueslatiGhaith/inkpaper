use std::string::String;

use crate::pool::testing::pool;
use crate::*;

struct Model {
    name: String,
}

struct Other;

struct View {
    model: Entity<Model>,
}

impl Render for View {
    fn render(&mut self, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let model = self.model.read(cx);
        // a listener after the read, so the read only needs a shared context
        let _ = cx.listener(|_: &mut View, _: &ActivateEvent, _| {});

        div().child(model.name.as_str())
    }
}

struct Setup {
    runtime: Runtime,
    model: Entity<Model>,
    other: Entity<Other>,
}

fn setup() -> Setup {
    let mut runtime = Runtime::new(pool());
    let model = runtime
        .create(|_| Model {
            name: String::from("hello"),
        })
        .unwrap();
    let other = runtime.create(|_| Other).unwrap();

    runtime.create_root(|_| View { model }).unwrap();
    runtime.set_partial_rebuilds(true);
    runtime.rebuild().unwrap();

    Setup {
        runtime,
        model,
        other,
    }
}

#[test]
fn a_reference_keeps_its_entity_borrowed_until_its_context_drops() {
    let Setup {
        runtime,
        model,
        other,
    } = setup();

    runtime
        .update(other, |_, cx| {
            let name = &model.read(cx).name;

            assert_eq!(
                model.try_update(cx, |_, _| ()),
                Err(EntityAccessError::BorrowConflict),
            );
            assert_eq!(name, "hello");
            assert_eq!(runtime.lease_count(), 1);
        })
        .unwrap();

    assert_eq!(runtime.lease_count(), 0);
    runtime
        .update(model, |model, _| model.name.push('!'))
        .unwrap();
}

#[test]
fn reading_an_entity_again_through_one_context_reuses_its_lease() {
    let Setup {
        runtime,
        model,
        other,
    } = setup();

    runtime
        .update(other, |_, cx| {
            let first = model.read(cx);
            let second = model.read(cx);

            assert!(core::ptr::eq(first, second));
            assert_eq!(runtime.lease_count(), 1);
        })
        .unwrap();

    assert_eq!(runtime.lease_count(), 0);
}

#[test]
fn a_reference_read_through_an_outer_context_outlives_an_inner_one() {
    let Setup {
        runtime,
        model,
        other,
    } = setup();
    let third = runtime.create(|_| Other).unwrap();

    runtime
        .update(other, |_, outer| {
            // the reference is read inside the inner update, but belongs to `outer`
            let name = third.update(outer, |_, _inner| &model.read(outer).name);

            // the inner context dropped, and the entity is still borrowed
            assert_eq!(runtime.lease_count(), 1);
            assert_eq!(
                model.try_update(outer, |_, _| ()),
                Err(EntityAccessError::BorrowConflict),
            );
            assert_eq!(name, "hello");
        })
        .unwrap();

    assert_eq!(runtime.lease_count(), 0);
}

#[test]
fn reads_of_the_inner_context_are_released_when_it_drops() {
    let Setup {
        runtime,
        model,
        other,
    } = setup();
    let third = runtime.create(|_| Other).unwrap();

    runtime
        .update(other, |_, outer| {
            third.update(outer, |_, inner| {
                let _ = model.read(inner);

                assert_eq!(runtime.lease_count(), 1);
            });

            assert_eq!(runtime.lease_count(), 0);
            model.update(outer, |model, _| model.name.push('!'));
        })
        .unwrap();
}

#[test]
fn a_render_can_borrow_what_it_read_until_its_element_is_mounted() {
    let Setup {
        mut runtime, model, ..
    } = setup();

    assert_eq!(runtime.frame_text_bytes_used(), "hello".len());
    assert_eq!(runtime.lease_count(), 0);

    // the read is recorded, so changing the model renders the view again
    runtime
        .update(model, |model, cx| {
            model.name = String::from("goodbye");
            cx.notify();
        })
        .unwrap();
    assert!(runtime.is_dirty());

    runtime.rebuild().unwrap();

    assert_eq!(runtime.frame_text_bytes_used(), "goodbye".len());
    assert_eq!(runtime.lease_count(), 0);
}
