use super::*;
use crate::*;
use core::cell::Cell;
use std::{rc::Rc, string::String, vec, vec::Vec};

#[derive(Debug, Clone, PartialEq, Eq)]
enum Draw {
    Text(Rect, Option<Rect>, String, ResolvedTextStyle),
    Image(Rect, Option<Rect>, ImagePaint),
    Box(Rect, Option<Rect>),
    Shapes(Rect, Option<Rect>),
}

#[derive(Default)]
struct Recorder {
    draws: Vec<Draw>,
    fail: bool,
}

impl Recorder {
    fn record(&mut self, draw: Draw) -> Result<(), &'static str> {
        self.draws.push(draw);
        if self.fail {
            Err("backend failure")
        } else {
            Ok(())
        }
    }
}

impl TextMeasurer for Recorder {
    fn measure_text(&self, _: &str, _: ResolvedTextStyle, _: Size) -> Size {
        panic!("custom canvas must not measure its painted text during layout");
    }
}

impl Painter for Recorder {
    type Error = &'static str;

    fn draw_box(
        &mut self,
        bounds: Rect,
        _: BoxPaint,
        clip: Option<Rect>,
    ) -> Result<(), Self::Error> {
        self.record(Draw::Box(bounds, clip))
    }

    fn draw_canvas(
        &mut self,
        bounds: Rect,
        clip: Option<Rect>,
        _: &mut dyn FnMut(Rect, &mut dyn CanvasPainter),
    ) -> Result<(), Self::Error> {
        self.record(Draw::Shapes(bounds, clip))
    }
}

impl ResourcePainter for Recorder {
    fn draw_text(
        &mut self,
        _: &mut (),
        text: &str,
        bounds: Rect,
        style: ResolvedTextStyle,
        clip: Option<Rect>,
    ) -> Result<(), Self::Error> {
        self.record(Draw::Text(bounds, clip, String::from(text), style))
    }

    fn draw_image(
        &mut self,
        _: &mut (),
        _: ImageSource,
        bounds: Rect,
        paint: ImagePaint,
        clip: Option<Rect>,
    ) -> Result<(), Self::Error> {
        self.record(Draw::Image(bounds, clip, paint))
    }
}

type TestRuntime = Runtime<1024, 4, 1024, 8, 16, 64, 8>;

fn rect(x: i32, y: i32, width: i32, height: i32) -> Rect {
    Rect::new(Point::new(px(x), px(y)), Size::new(px(width), px(height)))
}

#[test]
fn canvas_paint_preserves_local_bounds_clips_styles_and_damage() {
    struct View;

    impl Render for View {
        fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
            div()
                .relative()
                .left(px(10))
                .top(px(20))
                .w(px(40))
                .h(px(30))
                .overflow_hidden()
                .text_color(Color::RED)
                .child(
                    cx.canvas(|_, paint| {
                        assert_eq!(paint.bounds(), rect(0, 0, 40, 30));
                        paint.with_clip(rect(4, 5, 10, 10), |paint| {
                            paint.with_clip(rect(-100, -100, 300, 300), |paint| {
                                paint.draw_text(
                                    rect(0, 0, 40, 30),
                                    text("inside").font_size(px(18)),
                                );
                            });
                            paint.with_clip(rect(80, 80, 5, 5), |paint| {
                                paint.with_clip(rect(0, 0, 40, 30), |paint| {
                                    paint.draw_text(rect(0, 0, 40, 30), text("invisible"));
                                });
                            });
                        });
                        // Leaving a scoped clip restores the original canvas clip.
                        paint.draw_text(rect(35, 25, 10, 10), text("edge"));
                        paint.draw_text(rect(0, 0, -1, 10), text("empty"));
                    })
                    .size(Size::new(px(40), px(30))),
                )
        }
    }

    let mut runtime = TestRuntime::default();
    runtime.create_root(|_| View).unwrap();
    runtime.rebuild().unwrap();

    let mut recorder = Recorder::default();
    runtime.layout_with_measurer(Size::new(px(100), px(100)), &recorder);

    assert_eq!(runtime.frame_node_count(), 3);
    assert_eq!(runtime.frame_text_bytes_used(), 0);

    let report = runtime.paint(&mut recorder).unwrap().unwrap();
    let text_draws: Vec<_> = recorder
        .draws
        .iter()
        .filter(|draw| matches!(draw, Draw::Text(..)))
        .cloned()
        .collect();

    assert_eq!(
        text_draws,
        vec![
            Draw::Text(
                rect(10, 20, 40, 30),
                Some(rect(14, 25, 10, 10)),
                String::from("inside"),
                ResolvedTextStyle {
                    color: Color::RED,
                    font_size: px(18),
                    ..ResolvedTextStyle::default()
                }
            ),
            Draw::Text(
                rect(45, 45, 10, 10),
                Some(rect(45, 45, 5, 5)),
                String::from("edge"),
                ResolvedTextStyle {
                    color: Color::RED,
                    ..ResolvedTextStyle::default()
                }
            ),
        ]
    );

    assert!(report.content().has_text());
    assert!(!report.content().has_graphics());

    recorder.draws.clear();

    let damage = DamageRegion::from_rect(rect(16, 27, 2, 3));
    runtime.paint_with_damage(damage, &mut recorder).unwrap();

    let text_draws: Vec<_> = recorder
        .draws
        .into_iter()
        .filter(|draw| matches!(draw, Draw::Text(..)))
        .collect();

    assert_eq!(
        text_draws,
        vec![Draw::Text(
            rect(10, 20, 40, 30),
            Some(rect(16, 27, 2, 3)),
            String::from("inside"),
            ResolvedTextStyle {
                color: Color::RED,
                font_size: px(18),
                ..ResolvedTextStyle::default()
            },
        )]
    );
}

#[test]
fn canvas_paint_reports_images_and_reads_updated_entity_state() {
    struct View {
        value: &'static str,
    }

    impl Render for View {
        fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
            cx.canvas(|view, paint| {
                paint.draw_text(rect(0, 0, 20, 10), text(view.value));

                let source = ImageSource::new(ImageId::new(0), Size::new(px(10), px(10)));

                paint.draw_image(
                    rect(0, 10, 10, 10),
                    source,
                    ImagePaint {
                        color_mode: ImageColorMode::Monochrome,
                        ..ImagePaint::DEFAULT
                    },
                );
                paint.draw_image(
                    rect(10, 10, 10, 10),
                    source,
                    ImagePaint {
                        color_mode: ImageColorMode::Grayscale,
                        ..ImagePaint::DEFAULT
                    },
                );
            })
            .size(Size::new(px(40), px(30)))
        }
    }

    let mut runtime = TestRuntime::default();
    let view = runtime.create_root(|_| View { value: "old" }).unwrap();
    let mut recorder = Recorder::default();

    for value in ["first", "second"] {
        runtime
            .update(view, |view, cx| {
                view.value = value;
                cx.notify();
            })
            .unwrap();

        runtime.rebuild().unwrap();
        runtime.layout_with_measurer(Size::new(px(40), px(30)), &recorder);
        recorder.draws.clear();

        let report = runtime.paint(&mut recorder).unwrap().unwrap();

        assert!(matches!(&recorder.draws[0], Draw::Text(_, _, text, _) if text == value));
        assert_eq!(runtime.frame_node_count(), 2);
        assert!(report.content().has_text());
        assert!(report.content().has_monochrome_images());
        assert!(report.content().has_continuous_tone_images());
        assert!(!report.content().has_graphics());
    }
}

#[test]
fn canvas_paint_returns_first_backend_error_and_stops_drawing() {
    struct View {
        first: usize,
    }

    impl Render for View {
        fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
            cx.canvas(|view, paint| {
                for step in 0..4 {
                    match (view.first + step) % 4 {
                        0 => paint.draw_text(rect(0, 0, 10, 10), text("failure")),
                        1 => paint.draw_image(
                            rect(0, 0, 10, 10),
                            ImageSource::new(ImageId::new(0), Size::new(px(10), px(10))),
                            ImagePaint::DEFAULT,
                        ),
                        2 => paint.draw_box(
                            rect(0, 0, 10, 10),
                            BoxPaint {
                                background: Some(Color::RED),
                                ..BoxPaint::default()
                            },
                        ),
                        _ => paint.draw_shapes(rect(0, 0, 10, 10), |_, _| {}),
                    }
                }
            })
            .size(Size::new(px(20), px(20)))
        }
    }

    for first in 0..4 {
        let mut runtime = TestRuntime::default();
        runtime.create_root(|_| View { first }).unwrap();
        runtime.rebuild().unwrap();

        let mut recorder = Recorder {
            fail: true,
            ..Recorder::default()
        };

        runtime.layout_with_measurer(Size::new(px(20), px(20)), &recorder);

        assert_eq!(runtime.paint(&mut recorder), Err("backend failure"));
        assert_eq!(recorder.draws.len(), 1);
    }
}

#[test]
fn canvas_paint_callbacks_validate_kind_borrow_and_generation_and_drop_captures() {
    use crate::callback::CanvasInvokeError;
    use crate::callback::{
        CallbackArena, CallbackStore, register_canvas_callback, register_listener,
    };
    use crate::entity::{EntityBorrowKind, RawEntityBorrow};
    use core::any::TypeId;

    struct Capture(Rc<Cell<usize>>);

    impl Drop for Capture {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    let entities = EntityArena::<256, 2>::default();
    let entity = entities.insert(42u32).unwrap();
    let mut callbacks = CallbackArena::<256, 2>::default();
    let drops = Rc::new(Cell::new(0));
    let capture = Capture(drops.clone());

    let callback = register_canvas_callback(&callbacks, entity, move |value, paint| {
        assert_eq!(*value, 42);
        let _ = &capture;
        paint.draw_text(rect(0, 0, 10, 10), text("ok"));
    })
    .unwrap();

    let listener =
        register_listener(&callbacks, entity, |_: &mut u32, _: &ActivateEvent, _| {}).unwrap();

    let mut recorder = Recorder::default();
    let mut resources = ();
    let mut report = PaintReport::default();

    let mut sink = CanvasPaintSink {
        painter: &mut recorder,
        resources: &mut resources,
        report: &mut report,
        error: None,
    };

    let mut paint = PaintCx::new(
        &mut sink,
        rect(0, 0, 20, 20),
        None,
        ResolvedTextStyle::default(),
    );

    callbacks
        .invoke_canvas(callback, &mut paint, &entities)
        .unwrap();

    assert_eq!(
        callbacks.invoke_canvas(listener.id, &mut paint, &entities),
        Err(CanvasInvokeError::CallbackKindMismatch)
    );

    let borrow = RawEntityBorrow::acquire(
        &entities,
        entity.entity_id(),
        TypeId::of::<u32>(),
        EntityBorrowKind::Exclusive,
    )
    .unwrap();

    assert_eq!(
        callbacks.invoke_canvas(callback, &mut paint, &entities),
        Err(CanvasInvokeError::Entity(EntityAccessError::BorrowConflict))
    );

    drop(borrow);
    callbacks.reset();

    assert_eq!(drops.get(), 1);
    assert_eq!(
        callbacks.invoke_canvas(callback, &mut paint, &entities),
        Err(CanvasInvokeError::InvalidCallback)
    );

    let replacement = register_canvas_callback(&callbacks, entity, |_, _| {}).unwrap();

    assert_ne!(callback, replacement);
    assert_eq!(
        callbacks.invoke_canvas(callback, &mut paint, &entities),
        Err(CanvasInvokeError::InvalidCallback)
    );

    callbacks
        .invoke_canvas(replacement, &mut paint, &entities)
        .unwrap();
}

#[test]
#[cfg(not(feature = "alloc"))]
fn canvas_paint_fixed_storage_exhaustion_drops_captures() {
    use crate::callback::{CallbackArena, register_canvas_callback};

    struct Capture(Rc<Cell<usize>>);

    impl Drop for Capture {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    let entities = EntityArena::<64, 1>::default();
    let entity = entities.insert(()).unwrap();
    let drops = Rc::new(Cell::new(0));
    let capture = Capture(drops.clone());
    let slots = CallbackArena::<64, 0>::default();

    assert_eq!(
        register_canvas_callback(&slots, entity, move |_, _| {
            let _ = &capture;
        }),
        Err(CallbackAllocError::SlotsFull)
    );
    assert_eq!(drops.get(), 1);

    let capture = Capture(drops.clone());
    let bytes = CallbackArena::<1, 1>::default();

    assert_eq!(
        register_canvas_callback(&bytes, entity, move |_, _| {
            let _ = &capture;
        }),
        Err(CallbackAllocError::StorageFull)
    );
    assert_eq!(drops.get(), 2);
}

#[test]
fn static_canvas_paints_text_without_runtime_callback_context() {
    fn draw(paint: &mut PaintCx<'_>) {
        paint.draw_text(paint.bounds(), text("static"));
    }

    let mut frame = FrameArena::<4, 0>::default();
    let globals = GlobalArena::<0, 0>::default();
    let root = frame
        .mount(
            canvas(draw).size(Size::new(px(20), px(10))),
            AppContext::from_globals(&globals),
        )
        .unwrap();

    let mut recorder = Recorder::default();
    frame.layout(root, Size::new(px(20), px(10)), &recorder);

    let report = frame.paint(root, &mut recorder).unwrap();

    assert_eq!(
        recorder.draws,
        vec![Draw::Text(
            rect(0, 0, 20, 10),
            Some(rect(0, 0, 20, 10)),
            String::from("static"),
            ResolvedTextStyle::default(),
        )]
    );
    assert!(report.content().has_text());
    assert!(!report.content().has_graphics());
}
