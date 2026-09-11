use super::*;
use crate::{
    ActivateEvent, Context, EntityAccessError, EntityArena, GlobalArena, PaintCx, Point, Rect,
    ResolvedTextStyle, Size,
    callback::{register_canvas_callback, register_listener},
    px, text,
};
use std::{cell::Cell, rc::Rc};

#[derive(Default)]
struct Painter(usize);

impl crate::PaintSink for Painter {
    fn text(&mut self, _: Rect, _: Rect, _: &str, _: crate::ResolvedTextStyle) {
        self.0 += 1;
    }

    fn image(&mut self, _: Rect, _: Rect, _: crate::ImageSource, _: crate::ImagePaint) {}

    fn box_paint(&mut self, _: Rect, _: Rect, _: crate::BoxPaint) {}

    fn shapes(&mut self, _: Rect, _: Rect, _: &mut dyn FnMut(Rect, &mut dyn crate::CanvasPainter)) {
    }
}

#[test]
fn listener_storage_remains_stable_while_dispatch_grows_both_callback_kinds() {
    #[repr(align(128))]
    struct Capture(u32);

    let entities = EntityArena::<0, 0>::default();
    let callbacks = CallbackArena::<0, 1>::default();
    let globals = GlobalArena::<0, 0>::default();
    let target = entities.insert(0u32).unwrap();
    let notified = Cell::new(false);
    let capture = Capture(42);

    let listener = register_listener(
        &callbacks,
        target,
        move |state: &mut u32, _: &ActivateEvent, cx: &mut Context<'_, u32>| {
            let address = &capture as *const Capture;
            assert_eq!(address as usize % 128, 0);

            for _ in 0..32 {
                cx.try_listener::<ActivateEvent, _>(|_, _, _| {}).unwrap();
                cx.try_canvas(|_, _| {}).unwrap();
            }

            assert_eq!(&capture as *const Capture, address);
            *state += capture.0;
        },
    )
    .unwrap();

    let address = callbacks.lookup(listener.id).unwrap().0;

    callbacks
        .invoke_listener(listener, &ActivateEvent, &entities, &globals, &notified)
        .unwrap();

    assert_eq!(callbacks.lookup(listener.id).unwrap().0, address);
    assert_eq!(entities.read(target, |value| *value), Ok(42));
}

#[test]
fn listener_and_canvas_validation_survive_slot_reuse() {
    let entities = EntityArena::<0, 0>::default();
    let mut callbacks = CallbackArena::<0, 0>::default();
    let globals = GlobalArena::<0, 0>::default();
    let target = entities.insert(42u32).unwrap();
    let notified = Cell::new(false);

    let listener =
        register_listener(&callbacks, target, |_: &mut u32, _: &ActivateEvent, _| {}).unwrap();

    let canvas = register_canvas_callback(&callbacks, target, |value, paint| {
        assert_eq!(*value, 42);
        paint.draw_text(paint.bounds(), text("ok"));
    })
    .unwrap();

    let mut painter = Painter::default();
    let bounds = Rect::new(Point::ZERO, Size::new(px(10), px(10)));
    let mut paint = PaintCx::new(&mut painter, bounds, None, ResolvedTextStyle::default());

    callbacks
        .invoke_canvas(canvas, &mut paint, &entities)
        .unwrap();

    assert_eq!(
        callbacks.invoke_listener(
            Listener::<u32>::from_id(listener.id),
            &0,
            &entities,
            &globals,
            &notified
        ),
        Err(ListenerInvokeError::EventTypeMismatch)
    );

    assert_eq!(
        callbacks.invoke_canvas(listener.id, &mut paint, &entities),
        Err(CanvasInvokeError::CallbackKindMismatch)
    );

    assert_eq!(
        callbacks.invoke_listener(
            Listener::<ActivateEvent>::from_id(canvas),
            &ActivateEvent,
            &entities,
            &globals,
            &notified
        ),
        Err(ListenerInvokeError::InvalidListener)
    );

    entities
        .update(target, |_| {
            assert_eq!(
                callbacks.invoke_canvas(canvas, &mut paint, &entities),
                Err(CanvasInvokeError::Entity(EntityAccessError::BorrowConflict))
            );
        })
        .unwrap();

    callbacks.reset();

    let replacement = register_canvas_callback(&callbacks, target, |_, _| {}).unwrap();

    assert_eq!(replacement.slot(), listener.id.slot());
    assert_ne!(replacement.generation(), listener.id.generation());

    assert_eq!(
        callbacks.invoke_listener(listener, &ActivateEvent, &entities, &globals, &notified),
        Err(ListenerInvokeError::InvalidListener)
    );

    assert_eq!(
        callbacks.invoke_canvas(canvas, &mut paint, &entities),
        Err(CanvasInvokeError::InvalidCallback)
    );

    callbacks
        .invoke_canvas(replacement, &mut paint, &entities)
        .unwrap();
    assert_eq!(painter.0, 1);
}

#[test]
fn reset_releases_captures_and_reuses_slot_capacity() {
    struct Capture(Rc<Cell<usize>>);

    impl Drop for Capture {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    let entities = EntityArena::<0, 0>::default();
    let target = entities.insert(()).unwrap();
    let drops = Rc::new(Cell::new(0));
    let mut callbacks = CallbackArena::<0, 1>::default();

    for _ in 0..16 {
        let capture = Capture(drops.clone());

        register_listener(
            &callbacks,
            target,
            move |_: &mut (), _: &ActivateEvent, _| {
                let _ = &capture;
            },
        )
        .unwrap();

        let capture = Capture(drops.clone());

        register_canvas_callback(&callbacks, target, move |_, _| {
            let _ = &capture;
        })
        .unwrap();
    }

    let capacity = callbacks.entries.borrow().capacity();

    assert_eq!(drops.get(), 0);

    callbacks.reset();

    assert_eq!(drops.get(), 32);
    assert_eq!(callbacks.entries.borrow().capacity(), capacity);
    assert!(callbacks.entries.borrow().is_empty());

    let capture = Capture(drops.clone());

    register_listener(
        &callbacks,
        target,
        move |_: &mut (), _: &ActivateEvent, _| {
            let _ = &capture;
        },
    )
    .unwrap();

    drop(callbacks);

    assert_eq!(drops.get(), 33);
}

#[test]
#[cfg_attr(miri, ignore)]
fn callback_id_limit_is_enforced_and_reset_recovers_capacity() {
    let entities = EntityArena::<0, 0>::default();
    let target = entities.insert(()).unwrap();
    let mut callbacks = CallbackArena::<0, 0>::default();

    for _ in 0..=u16::MAX {
        register_listener(&callbacks, target, |_: &mut (), _: &ActivateEvent, _| {}).unwrap();
    }

    assert!(matches!(
        register_listener(&callbacks, target, |_: &mut (), _: &ActivateEvent, _| {}),
        Err(CallbackAllocError::SlotsFull)
    ));

    callbacks.reset();

    register_listener(&callbacks, target, |_: &mut (), _: &ActivateEvent, _| {}).unwrap();
}
