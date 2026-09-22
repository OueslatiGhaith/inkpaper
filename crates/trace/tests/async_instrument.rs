#![cfg(feature = "recording")]

use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicU32, AtomicU64, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
};

use inkpaper_trace::{SpanKind, capture, init, instrument};

static CYCLE_CLOCK: AtomicU32 = AtomicU32::new(0);

static MONOTONIC_CLOCK: AtomicU64 = AtomicU64::new(0);

fn cycle_clock() -> u32 {
    CYCLE_CLOCK.load(Ordering::Relaxed)
}

fn monotonic_clock() -> u64 {
    MONOTONIC_CLOCK.load(Ordering::Relaxed)
}

struct NoopWake;

impl Wake for NoopWake {
    fn wake(self: Arc<Self>) {}
}

struct YieldOnce {
    yielded: bool,
}

impl YieldOnce {
    const fn new() -> Self {
        Self { yielded: false }
    }
}

impl Future for YieldOnce {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        if self.yielded {
            Poll::Ready(())
        } else {
            self.yielded = true;

            Poll::Pending
        }
    }
}

#[instrument(
    target = "test.async",
    fields(value = value),
)]
async fn async_work(value: u32) -> u32 {
    YieldOnce::new().await;

    CYCLE_CLOCK.store(190, Ordering::Relaxed);

    value * 2
}

#[test]
fn instrumented_async_function_crosses_capture_boundary() {
    CYCLE_CLOCK.store(100, Ordering::Relaxed);

    MONOTONIC_CLOCK.store(0, Ordering::Relaxed);

    init(cycle_clock, 240_000_000, monotonic_clock, 1_000_000);

    CYCLE_CLOCK.store(110, Ordering::Relaxed);

    let mut future = Box::pin(async_work(9));

    let waker = Waker::from(Arc::new(NoopWake));

    let mut context = Context::from_waker(&waker);

    assert_eq!(future.as_mut().poll(&mut context), Poll::Pending);

    CYCLE_CLOCK.store(140, Ordering::Relaxed);

    let first_capture = capture().unwrap();

    // The async function has started,
    // but its lifetime is not complete.
    assert_eq!(first_capture.spans(), 0);

    drop(first_capture);

    assert_eq!(future.as_mut().poll(&mut context), Poll::Ready(18));

    let second_capture = capture().unwrap();

    assert_eq!(second_capture.spans(), 1);

    let span = second_capture.span(0).unwrap();

    assert_eq!(span.kind(), SpanKind::Async);

    assert_eq!(span.metadata().target(), "test.async");

    assert_eq!(span.metadata().name(), "async_work");

    assert_eq!(span.start_cycles(), 10);

    assert_eq!(span.duration_cycles(), 80);

    assert_eq!(span.field(0).unwrap().value().as_u32(), Some(9));
}
