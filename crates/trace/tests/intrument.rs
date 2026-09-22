#![cfg(feature = "recording")]

use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use inkpaper_trace::{capture, init, instrument};

static CYCLE_CLOCK: AtomicU32 = AtomicU32::new(0);

static MONOTONIC_CLOCK: AtomicU64 = AtomicU64::new(0);

fn cycle_clock() -> u32 {
    CYCLE_CLOCK.load(Ordering::Relaxed)
}

fn monotonic_clock() -> u64 {
    MONOTONIC_CLOCK.load(Ordering::Relaxed)
}

#[instrument(
    target = "test.instrument",
    name = "work",
    fields(
        value = value,
        positive = value > 0,
    ),
)]
fn work(value: i32) -> i32 {
    CYCLE_CLOCK.store(140, Ordering::Relaxed);

    if value > 0 {
        return value + 1;
    }

    value
}

struct Worker;

impl Worker {
    #[instrument(
        target = "test.method",
        fields(value = value),
    )]
    fn double(&self, value: u32) -> u32 {
        CYCLE_CLOCK.store(180, Ordering::Relaxed);

        value * 2
    }
}

#[test]
fn instrument_records_functions_and_methods() {
    CYCLE_CLOCK.store(100, Ordering::Relaxed);

    MONOTONIC_CLOCK.store(0, Ordering::Relaxed);

    init(cycle_clock, 240_000_000, monotonic_clock, 1_000_000);

    CYCLE_CLOCK.store(110, Ordering::Relaxed);

    assert_eq!(work(7), 8);

    CYCLE_CLOCK.store(150, Ordering::Relaxed);

    assert_eq!(Worker.double(9), 18);

    let capture = capture().unwrap();

    assert_eq!(capture.spans(), 2);

    let function = capture.span(0).unwrap();

    assert_eq!(function.metadata().target(), "test.instrument");

    assert_eq!(function.metadata().name(), "work");

    assert_eq!(function.start_cycles(), 10);

    assert_eq!(function.duration_cycles(), 30);

    assert_eq!(function.field(0).unwrap().name(), "value");

    assert_eq!(function.field(0).unwrap().value().as_i32(), Some(7));

    assert_eq!(function.field(1).unwrap().name(), "positive");

    assert_eq!(function.field(1).unwrap().value().as_bool(), Some(true));

    let method = capture.span(1).unwrap();

    assert_eq!(method.metadata().target(), "test.method");

    // No explicit `name`, so the method name is used.
    assert_eq!(method.metadata().name(), "double");

    assert_eq!(method.start_cycles(), 50);

    assert_eq!(method.duration_cycles(), 30);

    assert_eq!(method.field(0).unwrap().value().as_u32(), Some(9));
}
