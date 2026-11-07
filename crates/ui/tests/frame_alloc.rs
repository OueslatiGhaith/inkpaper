#![cfg(feature = "alloc")]
// fixture sizes are small constants
#![allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]

use inkpaper_ui::*;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    convert::Infallible,
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};

std::thread_local! {
    static ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) };
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
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record_allocation();
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record_allocation();
        unsafe { System.realloc(ptr, layout, size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

/// allocations from the heap and from `runtime`'s pool while `operation` runs
fn runtime_allocations<R>(
    runtime: &mut Runtime<R>,
    operation: impl FnOnce(&mut Runtime<R>),
) -> usize {
    let pooled = runtime.memory().allocations;
    let heap = allocations_during(|| operation(runtime));

    heap + runtime.memory().allocations - pooled
}

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
    fn measure_text(&self, text: &str, _: ResolvedTextStyle, _: AvailableSpace) -> Size {
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

    fn draw_glyph(
        &mut self,
        _: &GlyphBitmap<'_>,
        _: Point,
        _: Color,
        _: Rect,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn fill_rect(&mut self, _: Rect, _: Color, _: Rect) -> Result<(), Self::Error> {
        self.0 += 1;
        Ok(())
    }

    fn draw_pixels(&mut self, _: Point, _: &[Color]) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// paints each text as one filled rectangle
#[derive(Default)]
struct BlockText;

impl PaintResources for BlockText {
    fn paint_text<P: Painter>(
        &mut self,
        painter: &mut P,
        _: &str,
        bounds: Rect,
        _: ResolvedTextStyle,
        clip: Option<Rect>,
    ) -> Result<(), PaintError<P::Error>> {
        painter
            .fill_rect(bounds, Color::BLACK, clip.unwrap_or(bounds))
            .map_err(PaintError::Painter)
    }

    fn paint_image<P: Painter>(
        &mut self,
        _: &mut P,
        _: ImageSource,
        _: Rect,
        _: ImagePaint,
        _: Option<Rect>,
    ) -> Result<(), PaintError<P::Error>> {
        unreachable!("screen contains no images")
    }
}

#[test]
fn dynamic_runtime_allocates_on_growth_and_reuses_storage_for_rebuild_layout_and_paint() {
    let mut runtime = Runtime::<BlockText>::new(pool());

    let screen = runtime.create_root(|_| Screen).unwrap();

    assert!(runtime_allocations(&mut runtime, |runtime| runtime.rebuild().unwrap()) > 0);
    assert!(runtime.frame_node_count() > 1);
    assert_eq!(runtime.frame_text_bytes_used(), 10);

    let mut painter = TextCounter::default();

    let allocations = runtime_allocations(&mut runtime, |runtime| {
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

type StateRuntime = Runtime;

fn state_runtime() -> (StateRuntime, Entity<StatefulScreen>) {
    state_runtime_in(pool()).unwrap()
}

/// a runtime showing four rows, with the first focused and scrolled, or `None` when
/// they don't fit in `pool`
fn state_runtime_in(pool: Pool) -> Option<(StateRuntime, Entity<StatefulScreen>)> {
    let mut runtime = StateRuntime::new(pool);
    let screen = runtime.create_root(|_| StatefulScreen { rows: 4 }).ok()?;

    runtime.rebuild().ok()?;
    runtime
        .layout_with_measurer(Size::new(px(80), px(40)), &TextCounter::default())
        .unwrap();

    assert!(runtime.focus_next());
    assert!(runtime.scroll_at(Point::new(px(10), px(10)), Offset::new(px(0), px(10))));

    Some((runtime, screen))
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

    assert!(runtime_allocations(&mut runtime, |runtime| runtime.rebuild().unwrap()) > 0);
    assert_eq!(runtime.focused_target(), Some(original));

    let allocations = runtime_allocations(&mut runtime, |runtime| {
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
fn rebuilds_that_run_out_of_pool_can_be_retried() {
    let (mut probe, screen) = state_runtime();
    let header = 256 * 1024 - probe.memory().capacity;
    let four_rows = probe.memory().peak;

    probe
        .update(screen, |screen, cx| {
            screen.rows = 32;
            cx.notify();
        })
        .unwrap();
    probe.rebuild().unwrap();
    let thirty_two_rows = probe.memory().peak;

    // pools from too small for four rows to large enough for 32, so the rebuild runs out
    // in each table it grows
    let mut failures = 0;
    let mut recoveries = 0;
    for room in (four_rows..thirty_two_rows).step_by(64) {
        let pool = Pool::new(Box::leak(Box::<[u8]>::new_uninit_slice(header + room)));
        let Some((mut runtime, screen)) = state_runtime_in(pool) else {
            continue;
        };
        let original = runtime.focused_target().unwrap();

        runtime
            .update(screen, |screen, cx| {
                screen.rows = 32;
                cx.notify();
            })
            .unwrap();

        let Err(error) = runtime.rebuild() else {
            continue;
        };
        failures += 1;

        assert!(
            matches!(
                error,
                FrameBuildError::Mount(MountError::OutOfMemory(_))
                    | FrameBuildError::Identity(IdentityError::OutOfMemory(_))
            ),
            "{error:?}"
        );
        assert_eq!(runtime.frame_node_count(), 0);

        // the tables keep what they grew before running out, so four rows may no longer
        // fit in the smallest pools. Either way the runtime stays consistent
        runtime
            .update(screen, |screen, cx| {
                screen.rows = 4;
                cx.notify();
            })
            .unwrap();
        if runtime.rebuild().is_err() {
            assert_eq!(runtime.frame_node_count(), 0);
            continue;
        }
        recoveries += 1;

        runtime
            .layout_with_measurer(Size::new(px(80), px(40)), &TextCounter::default())
            .unwrap();

        assert!(runtime.scroll_at(Point::new(px(10), px(10)), Offset::new(px(0), px(-10))));
        assert!(runtime.focus_next());
        assert_eq!(runtime.focused_target(), Some(original));
    }

    assert!(failures > 0);
    assert!(recoveries > 0);
}

#[test]
fn growable_pools_rebuild_screens_their_first_chunk_does_not_fit() {
    let (mut runtime, screen) = state_runtime_in(Pool::growable(8 * 1024)).unwrap();
    let first = runtime.memory();

    runtime
        .update(screen, |screen, cx| {
            screen.rows = 32;
            cx.notify();
        })
        .unwrap();
    runtime.rebuild().unwrap();

    let grown = runtime.memory();
    assert!(grown.chunks_added > first.chunks_added);
    assert!(grown.capacity > first.capacity);
    assert!(runtime.frame_node_count() > 32);
}

#[test]
fn entity_allocation_failures_do_not_run_constructors() {
    let mut failures = 0;

    for runtime in growing_runtimes() {
        let called = Cell::new(false);
        let result = runtime.create(|_| {
            called.set(true);
            7u32
        });

        let Ok(entity) = result else {
            assert!(matches!(
                result,
                Err(EntityAllocError::OutOfMemory(OutOfMemory {
                    table: Table::Entities,
                    ..
                }))
            ));
            assert!(!called.get());
            failures += 1;
            continue;
        };

        assert_eq!(runtime.update(entity, |value, _| *value), Ok(7));
        break;
    }

    assert!(failures > 0);
}

#[test]
fn nested_entity_creation_and_constructor_unwind_preserve_existing_entities() {
    let runtime: Runtime = Runtime::new(pool());

    let parent = runtime
        .create(|cx| {
            let me = cx.entity();

            assert_eq!(me.try_read(cx, |_| ()), Err(EntityAccessError::NotReady));

            cx.new(|_| 42u32)
        })
        .unwrap();

    runtime
        .update(parent, |child, cx| {
            assert_eq!(child.read(cx, |value| *value), 42);

            for index in 0..32 {
                cx.new(|_| index);
            }

            assert_eq!(child.read(cx, |value| *value), 42);
        })
        .unwrap();

    let escaped = Cell::new(None);

    let result = catch_unwind(AssertUnwindSafe(|| {
        let _ = runtime.create::<u32>(|cx| {
            escaped.set(Some(cx.entity()));
            cx.new(|_| 99u32);
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
            assert_eq!(child.read(cx, |value| *value), 42)
        })
        .unwrap();
}

#[test]
fn callback_allocation_failures_drop_captures() {
    struct Capture(Rc<Cell<usize>>);

    impl Drop for Capture {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    for canvas in [false, true] {
        let mut failures = 0;

        for runtime in growing_runtimes() {
            let Ok(entity) = runtime.create(|_| ()) else {
                continue;
            };
            let drops = Rc::new(Cell::new(0));
            let capture = Capture(drops.clone());

            let result = runtime
                .update(entity, |_, cx| {
                    if canvas {
                        cx.try_canvas(move |_, _| {
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
                .unwrap();

            if result.is_err() {
                assert!(matches!(
                    result,
                    Err(CallbackAllocError::OutOfMemory(OutOfMemory {
                        table: Table::Callbacks,
                        ..
                    }))
                ));
                assert_eq!(drops.get(), 1);
                failures += 1;
                continue;
            }

            assert_eq!(drops.get(), 0);
            drop(runtime);
            assert_eq!(drops.get(), 1);
            break;
        }

        assert!(failures > 0);
    }
}

#[test]
fn globals_grow_without_moving_values_and_support_aligned_zsts() {
    #[repr(align(64))]
    struct Value<const N: usize>(u32);
    impl<const N: usize> Global for Value<N> {}

    #[repr(align(128))]
    struct Empty;
    impl Global for Empty {}

    let mut runtime: Runtime = Runtime::new(pool());

    runtime.set_global(Value::<0>(42)).unwrap();
    let address = {
        let value = runtime.global::<Value<0>>();
        (&*value) as *const Value<0> as usize
    };

    runtime.set_global(Value::<1>(1)).unwrap();
    runtime.set_global(Value::<2>(2)).unwrap();
    runtime.set_global(Value::<3>(3)).unwrap();
    runtime.set_global(Value::<4>(4)).unwrap();
    runtime.set_global(Empty).unwrap();

    assert_eq!(runtime.global_count(), 6);
    assert_eq!(
        (&*runtime.global::<Value<0>>()) as *const Value<0> as usize,
        address
    );
    assert_eq!(address % 64, 0);
    assert_eq!(
        (&*runtime.global::<Empty>()) as *const Empty as usize % 128,
        0
    );
    assert_eq!(runtime.global::<Value<0>>().0, 42);
    assert_eq!(runtime.global::<Value<4>>().0, 4);
}

#[test]
fn global_allocation_failures_drop_inputs_and_keep_existing_globals() {
    struct Value(Rc<Cell<usize>>);
    impl Global for Value {}

    impl Drop for Value {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    struct Other(#[allow(dead_code, reason = "it takes room")] [u8; 64]);
    impl Global for Other {}

    let mut first_failures = 0;
    let mut second_failures = 0;

    for mut runtime in growing_runtimes() {
        let drops = Rc::new(Cell::new(0));

        if runtime.set_global(Value(drops.clone())).is_err() {
            assert_eq!(drops.get(), 1);
            assert_eq!(runtime.global_count(), 0);
            assert!(!runtime.has_global::<Value>());
            first_failures += 1;
            continue;
        }

        let bytes = runtime.memory().table(Table::Globals);
        let result = runtime.set_global(Other([0; 64]));
        if result.is_ok() {
            break;
        }

        assert!(matches!(
            result,
            Err(GlobalSetError::OutOfMemory(OutOfMemory {
                table: Table::Globals,
                ..
            }))
        ));
        assert_eq!(runtime.global_count(), 1);
        assert_eq!(runtime.memory().table(Table::Globals), bytes);
        assert_eq!(runtime.global::<Value>().0.get(), 0);
        second_failures += 1;

        drop(runtime);
        assert_eq!(drops.get(), 1);
    }

    assert!(first_failures > 0);
    assert!(second_failures > 0);
}

#[test]
fn global_access_and_replacement_do_not_allocate() {
    struct Value(u32);
    impl Global for Value {}

    let mut runtime: Runtime = Runtime::new(pool());
    runtime.set_global(Value(1)).unwrap();
    let memory = runtime.memory();

    assert_eq!(
        allocations_during(|| {
            runtime.set_global(Value(2)).unwrap();
            assert_eq!(runtime.global::<Value>().0, 2);
            runtime.global_mut::<Value>().0 = 3;
            assert_eq!(runtime.global::<Value>().0, 3);
        }),
        0
    );
    assert_eq!(runtime.global_count(), 1);
    assert_eq!(runtime.memory(), memory);
}

#[test]
fn global_replacement_survives_a_panicking_destructor() {
    struct Value {
        panic_on_drop: bool,
        drops: Rc<Cell<usize>>,
    }
    impl Global for Value {}

    impl Drop for Value {
        fn drop(&mut self) {
            self.drops.set(self.drops.get() + 1);
            assert!(!self.panic_on_drop, "old global destructor");
        }
    }

    let drops = Rc::new(Cell::new(0));
    let mut runtime: Runtime = Runtime::new(pool());

    runtime
        .set_global(Value {
            panic_on_drop: true,
            drops: drops.clone(),
        })
        .unwrap();

    assert!(
        catch_unwind(std::panic::AssertUnwindSafe(|| {
            runtime
                .set_global(Value {
                    panic_on_drop: false,
                    drops: drops.clone(),
                })
                .unwrap();
        }))
        .is_err()
    );
    assert_eq!(drops.get(), 1);
    assert!(!runtime.global::<Value>().panic_on_drop);

    drop(runtime);
    assert_eq!(drops.get(), 2);
}

/// a pool large enough for this file's runtimes, over leaked memory
fn pool() -> inkpaper_ui::Pool {
    pool_of(256 * 1024)
}

fn pool_of(bytes: usize) -> Pool {
    Pool::new(Box::leak(Box::<[u8]>::new_uninit_slice(bytes)))
}

/// runtimes whose pools have room for 128 bytes, then 16 more each time, up to 4 KiB
fn growing_runtimes() -> impl Iterator<Item = Runtime> {
    let probe = 64 * 1024;
    let allocator = probe - pool_of(probe).stats().capacity;

    (128..4 * 1024)
        .step_by(16)
        .map(move |room| Runtime::new(pool_of(allocator + room)))
}
