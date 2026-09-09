#![cfg(feature = "alloc")]

use inkpaper_ui::*;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    convert::Infallible,
};

std::thread_local! {
    static ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) };
    static FAIL_AFTER: Cell<Option<usize>> = const { Cell::new(None) };
}

struct CountingAllocator;

fn record_allocation() {
    let _ = ALLOCATIONS.try_with(|count| {
        if let Some(value) = count.get() {
            count.set(Some(value + 1));
        }
    });
}

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record_allocation();
        if reject_allocation() {
            return core::ptr::null_mut();
        }
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record_allocation();
        if reject_allocation() {
            return core::ptr::null_mut();
        }
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record_allocation();
        if reject_allocation() {
            return core::ptr::null_mut();
        }
        unsafe { System.realloc(ptr, layout, size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

fn reject_allocation() -> bool {
    FAIL_AFTER
        .try_with(|remaining| match remaining.get() {
            Some(0) => {
                remaining.set(None);
                true
            }
            Some(count) => {
                remaining.set(Some(count - 1));
                false
            }
            None => false,
        })
        .unwrap_or(false)
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn allocations_during(operation: impl FnOnce()) -> usize {
    struct Reset;

    impl Drop for Reset {
        fn drop(&mut self) {
            ALLOCATIONS.with(|count| count.set(None));
        }
    }

    ALLOCATIONS.with(|count| count.set(Some(0)));
    let _reset = Reset;

    operation();

    ALLOCATIONS.with(|count| count.get().unwrap())
}

struct Screen;

impl Render for Screen {
    fn render<'a>(&'a mut self, _: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        div().child("Hello").child("World")
    }
}

#[derive(Default)]
struct TextCounter(usize);

impl TextMeasurer for TextCounter {
    fn measure_text(&self, text: &str, _: ResolvedTextStyle, _: Size) -> Size {
        Size::new(px(text.len() as i32 * 6), px(10))
    }
}

impl Painter for TextCounter {
    type Error = Infallible;

    fn draw_box(&mut self, _: Rect, _: BoxPaint, _: Option<Rect>) -> Result<(), Self::Error> {
        Ok(())
    }

    fn draw_canvas(
        &mut self,
        _: Rect,
        _: Option<Rect>,
        _: &mut dyn FnMut(Rect, &mut dyn CanvasPainter),
    ) -> Result<(), Self::Error> {
        unreachable!("screen contains no canvas")
    }
}

impl ResourcePainter for TextCounter {
    fn draw_text(
        &mut self,
        _: &mut (),
        _: &str,
        _: Rect,
        _: ResolvedTextStyle,
        _: Option<Rect>,
    ) -> Result<(), Self::Error> {
        self.0 += 1;
        Ok(())
    }

    fn draw_image(
        &mut self,
        _: &mut (),
        _: ImageSource,
        _: Rect,
        _: ImagePaint,
        _: Option<Rect>,
    ) -> Result<(), Self::Error> {
        unreachable!("screen contains no images")
    }
}

#[test]
fn dynamic_runtime_allocates_on_growth_and_reuses_storage_for_rebuild_layout_and_paint() {
    let mut runtime = RuntimeBuilder::default()
        .entities::<256, 4>()
        .callbacks::<256, 4>()
        .frame::<1, 1>()
        .element_states::<8>()
        .build();

    let screen = runtime.create_root(|_| Screen).unwrap();

    assert!(allocations_during(|| runtime.rebuild().unwrap()) > 0);
    assert!(runtime.frame_node_count() > 1);
    assert_eq!(runtime.frame_text_bytes_used(), 10);

    let mut painter = TextCounter::default();

    let allocations = allocations_during(|| {
        runtime
            .layout_with_measurer(Size::new(px(80), px(40)), &painter)
            .unwrap();
        runtime.paint(&mut painter).unwrap().unwrap();

        runtime.update(screen, |_, cx| cx.notify()).unwrap();
        runtime.rebuild().unwrap();

        runtime
            .layout_with_measurer(Size::new(px(80), px(40)), &painter)
            .unwrap();
        runtime.paint(&mut painter).unwrap().unwrap();
    });

    assert_eq!(allocations, 0);
    assert_eq!(painter.0, 4);

    runtime.shrink_frame_storage();
    runtime.paint(&mut painter).unwrap().unwrap();

    assert_eq!(painter.0, 6);
}

fn failing_allocation<T>(after: usize, operation: impl FnOnce() -> T) -> T {
    struct Reset;

    impl Drop for Reset {
        fn drop(&mut self) {
            FAIL_AFTER.with(|remaining| remaining.set(None));
        }
    }

    FAIL_AFTER.with(|remaining| remaining.set(Some(after)));
    let _reset = Reset;

    operation()
}

struct StatefulScreen {
    rows: usize,
}

impl Render for StatefulScreen {
    fn render<'a>(&'a mut self, _: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        div()
            .id("scroll")
            .w(px(80))
            .h(px(40))
            .overflow_y_scroll()
            .children((0..self.rows).map(|row| div().id(row).h(px(30)).focusable()))
    }
}

type StateRuntime = Runtime<256, 4, 256, 4, 128, 128, 1>;

fn state_runtime() -> (StateRuntime, Entity<StatefulScreen>) {
    let mut runtime = StateRuntime::default();
    let screen = runtime.create_root(|_| StatefulScreen { rows: 4 }).unwrap();

    runtime.rebuild().unwrap();
    runtime
        .layout_with_measurer(Size::new(px(80), px(40)), &TextCounter::default())
        .unwrap();

    assert!(runtime.focus_next());
    assert!(runtime.scroll_at(Point::new(px(10), px(10)), Offset::new(px(0), px(10))));

    (runtime, screen)
}

#[test]
fn dynamic_identity_growth_preserves_focus_and_scrolling_without_input_allocations() {
    let (mut runtime, screen) = state_runtime();
    let original = runtime.focused_target().unwrap();

    runtime
        .update(screen, |screen, cx| {
            screen.rows = 32;
            cx.notify();
        })
        .unwrap();

    assert!(allocations_during(|| runtime.rebuild().unwrap()) > 0);
    assert_eq!(runtime.focused_target(), Some(original));

    let allocations = allocations_during(|| {
        runtime
            .layout_with_measurer(Size::new(px(80), px(40)), &TextCounter::default())
            .unwrap();

        assert!(runtime.scroll_at(Point::new(px(10), px(10)), Offset::new(px(0), px(-10))));
        assert!(!runtime.scroll_at(Point::new(px(10), px(10)), Offset::new(px(0), px(-10))));
        assert!(runtime.focus_next());

        runtime.rebuild().unwrap();
        runtime
            .layout_with_measurer(Size::new(px(80), px(40)), &TextCounter::default())
            .unwrap();
    });

    assert_eq!(allocations, 0);

    runtime
        .update(screen, |screen, cx| {
            screen.rows = 0;
            cx.notify();
        })
        .unwrap();
    runtime.rebuild().unwrap();

    assert_eq!(runtime.focused_target(), None);

    runtime
        .update(screen, |screen, cx| {
            screen.rows = 4;
            cx.notify();
        })
        .unwrap();
    runtime.rebuild().unwrap();
    runtime
        .layout_with_measurer(Size::new(px(80), px(40)), &TextCounter::default())
        .unwrap();

    assert!(runtime.focus_next());
    assert_ne!(runtime.focused_target(), Some(original));
}

#[test]
fn every_identity_and_scroll_growth_allocation_can_fail_and_be_retried() {
    let (mut probe, screen) = state_runtime();

    probe
        .update(screen, |screen, cx| {
            screen.rows = 32;
            cx.notify();
        })
        .unwrap();

    let allocations = allocations_during(|| probe.rebuild().unwrap());
    assert!(allocations > 0);

    for after in 0..allocations {
        let (mut runtime, screen) = state_runtime();
        let original = runtime.focused_target().unwrap();

        runtime
            .update(screen, |screen, cx| {
                screen.rows = 32;
                cx.notify();
            })
            .unwrap();

        assert_eq!(
            failing_allocation(after, || runtime.rebuild()),
            Err(FrameBuildError::Identity(IdentityError::AllocationFailed))
        );
        assert_eq!(runtime.frame_node_count(), 0);

        runtime
            .update(screen, |screen, cx| {
                screen.rows = 4;
                cx.notify();
            })
            .unwrap();
        runtime.rebuild().unwrap();
        runtime
            .layout_with_measurer(Size::new(px(80), px(40)), &TextCounter::default())
            .unwrap();

        assert!(runtime.scroll_at(Point::new(px(10), px(10)), Offset::new(px(0), px(-10))));
        assert!(runtime.focus_next());
        assert_eq!(runtime.focused_target(), Some(original));

        runtime
            .update(screen, |screen, cx| {
                screen.rows = 32;
                cx.notify();
            })
            .unwrap();
        runtime.rebuild().unwrap();
    }
}

#[test]
fn entity_allocation_failures_do_not_run_constructors_and_allow_retry() {
    type SmallRuntime = Runtime<0, 0, 0, 0, 0, 0, 0>;

    let expected = SmallRuntime::default()
        .create(|_| 7u32)
        .unwrap()
        .entity_id();

    for after in 0..2 {
        let runtime = SmallRuntime::default();
        let called = Cell::new(false);

        let result = failing_allocation(after, || {
            runtime.create(|_| {
                called.set(true);
                7u32
            })
        });

        assert_eq!(result, Err(EntityAllocError::AllocationFailed));
        assert!(!called.get());

        let entity = runtime.create(|_| 9u32).unwrap();

        assert_eq!(entity.entity_id(), expected);
        assert_eq!(runtime.update(entity, |value, _| *value), Ok(9));
    }
}

#[test]
fn nested_entity_creation_and_constructor_unwind_preserve_existing_entities() {
    use std::panic::{AssertUnwindSafe, catch_unwind};

    let runtime = Runtime::<0, 1, 0, 0, 0, 0, 0>::default();

    let parent = runtime
        .create(|cx| {
            let me = cx.entity();

            assert_eq!(me.read(cx, |_| ()), Err(EntityAccessError::NotReady));

            cx.new(|_| 42u32).unwrap()
        })
        .unwrap();

    runtime
        .update(parent, |child, cx| {
            assert_eq!(child.read(cx, |value| *value), Ok(42));

            for index in 0..32 {
                cx.new(|_| index).unwrap();
            }

            assert_eq!(child.read(cx, |value| *value), Ok(42));
        })
        .unwrap();

    let escaped = Cell::new(None);

    let result = catch_unwind(AssertUnwindSafe(|| {
        let _ = runtime.create::<u32>(|cx| {
            escaped.set(Some(cx.entity()));
            cx.new(|_| 99u32).unwrap();
            panic!("constructor failure");
        });
    }));

    assert!(result.is_err());

    let escaped = escaped.get().unwrap();

    assert_eq!(
        runtime.update(escaped, |_, _| ()),
        Err(EntityAccessError::InvalidEntity)
    );

    let retry = runtime.create(|_| 5u32).unwrap();

    assert_ne!(escaped, retry);
    assert_eq!(runtime.update(retry, |value, _| *value), Ok(5));

    runtime
        .update(parent, |child, cx| {
            assert_eq!(child.read(cx, |value| *value), Ok(42))
        })
        .unwrap();
}

#[test]
fn callback_allocation_failures_drop_captures_and_allow_listener_and_canvas_retry() {
    use std::rc::Rc;

    struct Capture(Rc<Cell<usize>>);

    impl Drop for Capture {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    for canvas in [false, true] {
        for after in 0..2 {
            let runtime = Runtime::<0, 0, 0, 0, 0, 0, 0>::default();
            let entity = runtime.create(|_| ()).unwrap();
            let drops = Rc::new(Cell::new(0));
            let capture = Capture(drops.clone());

            let result = failing_allocation(after, || {
                runtime.update(entity, |_, cx| {
                    if canvas {
                        cx.try_canvas(move |_, _, _| {
                            let _ = &capture;
                        })
                        .map(|_| ())
                    } else {
                        cx.try_listener::<ActivateEvent, _>(move |_, _, _| {
                            let _ = &capture;
                        })
                        .map(|_| ())
                    }
                })
            })
            .unwrap();

            assert_eq!(result, Err(CallbackAllocError::AllocationFailed));
            assert_eq!(drops.get(), 1);

            let capture = Capture(drops.clone());

            runtime
                .update(entity, |_, cx| {
                    if canvas {
                        cx.try_canvas(move |_, _, _| {
                            let _ = &capture;
                        })
                        .unwrap();
                    } else {
                        cx.try_listener::<ActivateEvent, _>(move |_, _, _| {
                            let _ = &capture;
                        })
                        .unwrap();
                    }
                })
                .unwrap();

            assert_eq!(drops.get(), 1);

            drop(runtime);

            assert_eq!(drops.get(), 2);
        }
    }
}
