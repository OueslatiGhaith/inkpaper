use core::cell::RefCell;

#[cfg(target_has_atomic = "32")]
use core::sync::atomic::{AtomicU32, Ordering};

use critical_section::Mutex;

use crate::{
    Callsite, Field, Value, ValueKind,
    metadata::{MetricCallsite, MetricKind},
};

pub const TRACE_CAPACITY: usize = 256;
pub const METRIC_CAPACITY: usize = 32;

type CycleClockFn = fn() -> u32;
type MonotonicClockFn = fn() -> u64;

const CYCLE_WRAP: u64 = 1u64 << 32;

static EMPTY_FIELDS: [Field; 0] = [];
static EMPTY_CALLSITE: Callsite = Callsite::new("<empty>", "trace", &EMPTY_FIELDS);

static EMPTY_METRIC_CALLSITE: MetricCallsite =
    MetricCallsite::new("<empty>", "trace", MetricKind::Counter, "");

#[cfg(target_has_atomic = "32")]
static RECORDING_ENABLED: AtomicU32 = AtomicU32::new(0);

#[cfg(not(target_has_atomic = "32"))]
static RECORDING_ENABLED: Mutex<RefCell<bool>> = Mutex::new(RefCell::new(false));

#[derive(Debug, Clone, Copy)]
struct Clock {
    cycle_clock: CycleClockFn,
    cycle_hz: u32,

    monotonic_clock: MonotonicClockFn,
    monotonic_hz: u32,

    anchor_cycles: u32,
    anchor_monotonic: u64,
}

impl Clock {
    fn new(
        cycle_clock: CycleClockFn,
        cycle_hz: u32,
        monotonic_clock: MonotonicClockFn,
        monotonic_hz: u32,
    ) -> Self {
        let anchor_monotonic = monotonic_clock();

        let anchor_cycles = cycle_clock();

        Self {
            cycle_clock,
            cycle_hz,

            monotonic_clock,
            monotonic_hz,

            anchor_cycles,
            anchor_monotonic,
        }
    }

    #[inline(always)]
    fn now_cycles(self) -> u64 {
        let raw = (self.cycle_clock)();

        let monotonic = (self.monotonic_clock)();

        let raw_delta = u64::from(raw.wrapping_sub(self.anchor_cycles));

        let monotonic_delta = monotonic.wrapping_sub(self.anchor_monotonic);

        let coarse_cycles = u128::from(monotonic_delta).saturating_mul(u128::from(self.cycle_hz))
            / u128::from(self.monotonic_hz);

        let coarse_cycles = u64::try_from(coarse_cycles).unwrap_or(u64::MAX);

        unwrap_cycle_counter(raw_delta, coarse_cycles)
    }
}

#[inline(always)]
fn unwrap_cycle_counter(low_cycles: u64, coarse_cycles: u64) -> u64 {
    let epoch = coarse_cycles & !(CYCLE_WRAP - 1);

    let candidate = epoch | low_cycles;

    let mut best = candidate;
    let mut best_distance = candidate.abs_diff(coarse_cycles);

    if candidate >= CYCLE_WRAP {
        let lower = candidate - CYCLE_WRAP;

        let distance = lower.abs_diff(coarse_cycles);

        if distance < best_distance {
            best = lower;
            best_distance = distance;
        }
    }

    if let Some(upper) = candidate.checked_add(CYCLE_WRAP) {
        let distance = upper.abs_diff(coarse_cycles);

        if distance < best_distance {
            best = upper;
        }
    }

    best
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordedField {
    field: Field,
    value: Value,
}

impl RecordedField {
    pub const fn name(self) -> &'static str {
        self.field.name()
    }

    pub const fn value(self) -> Value {
        self.value
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TraceRecord {
    start_cycles: u64,
    duration_cycles: u64,

    value0: u32,
    value1: u32,

    callsite: &'static Callsite,

    depth: u8,
    value_kinds: u8,
}

#[cfg(target_pointer_width = "32")]
const _: () = {
    assert!(
        core::mem::size_of::<TraceRecord>() <= 32,
        "TraceRecord exceeded the 32-byte 32-bit target budget",
    );
};

impl TraceRecord {
    const EMPTY: Self = Self {
        start_cycles: 0,
        duration_cycles: 0,

        value0: 0,
        value1: 0,

        callsite: &EMPTY_CALLSITE,

        depth: 0,
        value_kinds: 0,
    };

    fn new(
        callsite: &'static Callsite,
        depth: u8,
        start_cycles: u64,
        duration_cycles: u64,
        values: [Value; 2],
    ) -> Self {
        let value_kinds = (values[0].kind() as u8) | ((values[1].kind() as u8) << 2);

        Self {
            start_cycles,
            duration_cycles,

            value0: values[0].raw(),
            value1: values[1].raw(),

            callsite,

            depth,
            value_kinds,
        }
    }

    pub const fn metadata(self) -> &'static crate::Metadata {
        self.callsite.metadata()
    }

    pub const fn depth(self) -> u8 {
        self.depth
    }

    pub const fn start_cycles(self) -> u64 {
        self.start_cycles
    }

    pub const fn duration_cycles(self) -> u64 {
        self.duration_cycles
    }

    pub fn field(self, index: usize) -> Option<RecordedField> {
        let field = *self.callsite.metadata().fields().get(index)?;

        let (raw, shift) = match index {
            0 => (self.value0, 0),
            1 => (self.value1, 2),
            _ => return None,
        };

        let kind = ValueKind::from_bits((self.value_kinds >> shift) & 0b11)?;

        Some(RecordedField {
            field,
            value: Value::from_raw(kind, raw),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CallsiteId(u16);

impl CallsiteId {
    pub const fn get(self) -> u16 {
        self.0
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CallsiteDefinition {
    id: CallsiteId,
    metadata: &'static crate::Metadata,
}

impl CallsiteDefinition {
    pub const fn id(self) -> CallsiteId {
        self.id
    }

    pub const fn metadata(self) -> &'static crate::Metadata {
        self.metadata
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CapturedSpan {
    callsite_id: CallsiteId,
    record: TraceRecord,
}

impl CapturedSpan {
    pub const fn callsite_id(self) -> CallsiteId {
        self.callsite_id
    }

    pub const fn metadata(self) -> &'static crate::Metadata {
        self.record.metadata()
    }

    pub const fn depth(self) -> u8 {
        self.record.depth()
    }

    pub const fn start_cycles(self) -> u64 {
        self.record.start_cycles()
    }

    pub const fn duration_cycles(self) -> u64 {
        self.record.duration_cycles()
    }

    pub fn field(self, index: usize) -> Option<RecordedField> {
        self.record.field(index)
    }
}

#[derive(Debug, Clone, Copy)]
struct MetricRecord {
    callsite: &'static MetricCallsite,

    count: u32,
    sum: u64,
    max: u32,
}

impl MetricRecord {
    const EMPTY: Self = Self {
        callsite: &EMPTY_METRIC_CALLSITE,
        count: 0,
        sum: 0,
        max: 0,
    };

    const fn new(callsite: &'static MetricCallsite) -> Self {
        Self {
            callsite,
            count: 0,
            sum: 0,
            max: 0,
        }
    }

    fn observe(&mut self, value: u32) {
        self.count = self.count.saturating_add(1);

        self.sum = self.sum.saturating_add(u64::from(value));

        self.max = self.max.max(value);
    }
}

#[cfg(target_pointer_width = "32")]
const _: () = {
    assert!(
        core::mem::size_of::<MetricRecord>() <= 24,
        "MetricRecord exceeded the 24-byte 32-bit target budget",
    );
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MetricId(u16);

impl MetricId {
    pub const fn get(self) -> u16 {
        self.0
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CapturedMetric {
    id: MetricId,
    record: MetricRecord,
}

impl CapturedMetric {
    pub const fn id(self) -> MetricId {
        self.id
    }

    pub const fn metadata(self) -> &'static crate::Metadata {
        self.record.callsite.metadata()
    }

    pub const fn kind(self) -> MetricKind {
        self.record.callsite.kind()
    }

    pub const fn unit(self) -> &'static str {
        self.record.callsite.unit()
    }

    pub const fn count(self) -> u32 {
        self.record.count
    }

    pub const fn sum(self) -> u64 {
        self.record.sum
    }

    pub const fn max(self) -> u32 {
        self.record.max
    }
}

struct TraceBuffer {
    records: [TraceRecord; TRACE_CAPACITY],

    record_len: usize,
    next_record: usize,
    overwritten: u64,

    metrics: [MetricRecord; METRIC_CAPACITY],

    metric_len: usize,
    metric_dropped: u32,
}

impl TraceBuffer {
    const fn new() -> Self {
        Self {
            records: [TraceRecord::EMPTY; TRACE_CAPACITY],

            record_len: 0,
            next_record: 0,
            overwritten: 0,

            metrics: [MetricRecord::EMPTY; METRIC_CAPACITY],

            metric_len: 0,
            metric_dropped: 0,
        }
    }

    fn reset(&mut self) {
        self.record_len = 0;
        self.next_record = 0;
        self.overwritten = 0;

        self.metric_len = 0;
        self.metric_dropped = 0;
    }

    fn push_span(&mut self, record: TraceRecord) {
        self.records[self.next_record] = record;

        self.next_record = (self.next_record + 1) % TRACE_CAPACITY;

        if self.record_len < TRACE_CAPACITY {
            self.record_len += 1;
        } else {
            self.overwritten = self.overwritten.saturating_add(1);
        }
    }

    fn record(&self, logical_index: usize) -> Option<TraceRecord> {
        if logical_index >= self.record_len {
            return None;
        }

        let oldest = if self.record_len < TRACE_CAPACITY {
            0
        } else {
            self.next_record
        };

        let physical_index = (oldest + logical_index) % TRACE_CAPACITY;

        self.records.get(physical_index).copied()
    }

    fn observe_metric(&mut self, callsite: &'static MetricCallsite, value: u32) {
        let existing = self.metrics[..self.metric_len]
            .iter()
            .position(|metric| core::ptr::eq(metric.callsite, callsite));

        let index = if let Some(index) = existing {
            index
        } else {
            if self.metric_len >= METRIC_CAPACITY {
                self.metric_dropped = self.metric_dropped.saturating_add(1);

                return;
            }

            let index = self.metric_len;

            self.metrics[index] = MetricRecord::new(callsite);

            self.metric_len = index + 1;

            index
        };

        self.metrics[index].observe(value);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FrozenCapture {
    buffer_index: usize,
    capture_id: u32,
}

struct TraceState {
    clock: Option<Clock>,

    generation: u32,
    depth: u8,

    active_buffer: usize,

    frozen: Option<FrozenCapture>,

    next_capture_id: u32,

    buffers: [TraceBuffer; 2],
}

impl TraceState {
    const fn new() -> Self {
        Self {
            clock: None,

            generation: 0,
            depth: 0,

            active_buffer: 0,

            frozen: None,

            next_capture_id: 0,

            buffers: [TraceBuffer::new(), TraceBuffer::new()],
        }
    }
}

static TRACE: Mutex<RefCell<TraceState>> = Mutex::new(RefCell::new(TraceState::new()));

#[cfg(target_has_atomic = "32")]
#[inline(always)]
fn set_recording_enabled(enabled: bool) {
    RECORDING_ENABLED.store(u32::from(enabled), Ordering::Relaxed);
}

#[cfg(not(target_has_atomic = "32"))]
#[inline(always)]
fn set_recording_enabled(enabled: bool) {
    critical_section::with(|cs| {
        *RECORDING_ENABLED.borrow(cs).borrow_mut() = enabled;
    });
}

#[cfg(target_has_atomic = "32")]
#[inline(always)]
pub(crate) fn is_recording() -> bool {
    RECORDING_ENABLED.load(Ordering::Relaxed) != 0
}

#[cfg(not(target_has_atomic = "32"))]
#[inline(always)]
pub(crate) fn is_recording() -> bool {
    critical_section::with(|cs| *RECORDING_ENABLED.borrow(cs).borrow())
}

pub fn init(
    cycle_clock: CycleClockFn,
    cycle_hz: u32,
    monotonic_clock: MonotonicClockFn,
    monotonic_hz: u32,
) {
    assert!(cycle_hz > 0, "trace cycle clock frequency must be non-zero");

    assert!(
        monotonic_hz > 0,
        "trace monotonic clock frequency must be non-zero",
    );

    let clock = Clock::new(cycle_clock, cycle_hz, monotonic_clock, monotonic_hz);

    critical_section::with(|cs| {
        let mut state = TRACE.borrow(cs).borrow_mut();

        let mut generation = state.generation.wrapping_add(1);

        if generation == 0 {
            generation = 1;
        }

        state.clock = Some(clock);

        state.generation = generation;

        state.depth = 0;

        state.active_buffer = 0;
        state.frozen = None;

        state.next_capture_id = 0;

        state.buffers[0].reset();

        state.buffers[1].reset();
    });

    set_recording_enabled(true);
}

pub struct TraceCapture {
    capture_id: u32,
    clock_hz: u32,

    captured_at_cycles: u64,

    spans: usize,
    overwritten: u64,

    metrics: usize,
    metric_dropped: u32,

    buffer_index: usize,
}

impl TraceCapture {
    pub const fn id(&self) -> u32 {
        self.capture_id
    }

    pub const fn clock_hz(&self) -> u32 {
        self.clock_hz
    }

    pub const fn captured_at_cycles(&self) -> u64 {
        self.captured_at_cycles
    }

    pub const fn spans(&self) -> usize {
        self.spans
    }

    pub const fn overwritten(&self) -> u64 {
        self.overwritten
    }

    pub const fn metrics(&self) -> usize {
        self.metrics
    }

    pub const fn metric_dropped(&self) -> u32 {
        self.metric_dropped
    }

    pub fn entry(&self, index: usize) -> Option<CaptureEntry> {
        critical_section::with(|cs| {
            let state = TRACE.borrow(cs).borrow();

            if !self.matches(&state) || index >= self.spans {
                return None;
            }

            let buffer = state.buffers.get(self.buffer_index)?;

            let record = buffer.record(index)?;

            let callsite_id = callsite_id_for(buffer, index)?;

            let definition = if usize::from(callsite_id.get()) == index {
                Some(CallsiteDefinition {
                    id: callsite_id,
                    metadata: record.metadata(),
                })
            } else {
                None
            };

            Some(CaptureEntry {
                definition,
                span: CapturedSpan {
                    callsite_id,
                    record,
                },
            })
        })
    }

    pub fn span(&self, index: usize) -> Option<CapturedSpan> {
        self.entry(index).map(CaptureEntry::span)
    }

    pub fn definition_at(&self, index: usize) -> Option<CallsiteDefinition> {
        self.entry(index).and_then(CaptureEntry::definition)
    }

    pub fn metric(&self, index: usize) -> Option<CapturedMetric> {
        critical_section::with(|cs| {
            let state = TRACE.borrow(cs).borrow();

            if !self.matches(&state) || index >= self.metrics {
                return None;
            }

            let buffer = state.buffers.get(self.buffer_index)?;

            let record = *buffer.metrics.get(index)?;

            let id = MetricId(u16::try_from(index).ok()?);

            Some(CapturedMetric { id, record })
        })
    }

    fn matches(&self, state: &TraceState) -> bool {
        state.frozen
            == Some(FrozenCapture {
                buffer_index: self.buffer_index,
                capture_id: self.capture_id,
            })
    }
}

impl Drop for TraceCapture {
    fn drop(&mut self) {
        critical_section::with(|cs| {
            let mut state = TRACE.borrow(cs).borrow_mut();

            let frozen = FrozenCapture {
                buffer_index: self.buffer_index,
                capture_id: self.capture_id,
            };

            if state.frozen != Some(frozen) {
                return;
            }

            state.buffers[self.buffer_index].reset();

            state.frozen = None;
        });
    }
}

pub fn capture() -> Option<TraceCapture> {
    if !is_recording() {
        return None;
    }

    critical_section::with(|cs| {
        let mut state = TRACE.borrow(cs).borrow_mut();

        if state.frozen.is_some() {
            return None;
        }

        let clock = state.clock?;

        let captured_at_cycles = clock.now_cycles();

        let frozen_buffer = state.active_buffer;

        let next_buffer = frozen_buffer ^ 1;

        state.buffers[next_buffer].reset();

        state.active_buffer = next_buffer;

        let mut capture_id = state.next_capture_id.wrapping_add(1);

        if capture_id == 0 {
            capture_id = 1;
        }

        state.next_capture_id = capture_id;

        let buffer = &state.buffers[frozen_buffer];

        let capture = TraceCapture {
            capture_id,
            clock_hz: clock.cycle_hz,

            captured_at_cycles,

            spans: buffer.record_len,
            overwritten: buffer.overwritten,

            metrics: buffer.metric_len,
            metric_dropped: buffer.metric_dropped,

            buffer_index: frozen_buffer,
        };

        state.frozen = Some(FrozenCapture {
            buffer_index: frozen_buffer,
            capture_id,
        });

        Some(capture)
    })
}

#[derive(Debug, Clone, Copy)]
pub struct CaptureEntry {
    definition: Option<CallsiteDefinition>,

    span: CapturedSpan,
}

impl CaptureEntry {
    pub const fn definition(self) -> Option<CallsiteDefinition> {
        self.definition
    }

    pub const fn span(self) -> CapturedSpan {
        self.span
    }
}

fn callsite_id_for(buffer: &TraceBuffer, index: usize) -> Option<CallsiteId> {
    let record = buffer.record(index)?;

    for candidate_index in 0..=index {
        let candidate = buffer.record(candidate_index)?;

        if core::ptr::eq(candidate.callsite, record.callsite) {
            return Some(CallsiteId(u16::try_from(candidate_index).ok()?));
        }
    }

    None
}

#[inline(always)]
pub(crate) fn observe_metric(callsite: &'static MetricCallsite, value: u32) {
    record_metric(callsite, value, None);
}

fn observe_metric_for_generation(generation: u32, callsite: &'static MetricCallsite, value: u32) {
    record_metric(callsite, value, Some(generation));
}

fn record_metric(callsite: &'static MetricCallsite, value: u32, expected_generation: Option<u32>) {
    if !is_recording() {
        return;
    }

    critical_section::with(|cs| {
        let mut state = TRACE.borrow(cs).borrow_mut();

        if state.clock.is_none() {
            return;
        }

        if let Some(generation) = expected_generation
            && state.generation != generation
        {
            return;
        }

        let active = state.active_buffer;

        state.buffers[active].observe_metric(callsite, value);
    });
}

pub struct Span {
    generation: u32,

    callsite: &'static Callsite,

    clock: Option<Clock>,
    start_cycles: u64,

    values: [Value; 2],

    depth: u8,
}

impl Span {
    pub const fn disabled() -> Self {
        Self {
            generation: 0,

            callsite: &EMPTY_CALLSITE,

            clock: None,
            start_cycles: 0,

            values: [Value::EMPTY; 2],

            depth: 0,
        }
    }

    #[inline(always)]
    pub(crate) fn start(callsite: &'static Callsite, values: [Value; 2]) -> Self {
        if !is_recording() {
            return Self::disabled();
        }

        critical_section::with(|cs| {
            let mut state = TRACE.borrow(cs).borrow_mut();

            let Some(clock) = state.clock else {
                return Self::disabled();
            };

            let depth = state.depth;

            state.depth = state.depth.saturating_add(1);

            Self {
                generation: state.generation,

                callsite,

                clock: Some(clock),

                start_cycles: clock.now_cycles(),

                values,

                depth,
            }
        })
    }
}

impl Drop for Span {
    #[inline(always)]
    fn drop(&mut self) {
        let Some(clock) = self.clock else {
            return;
        };

        let ended_at = clock.now_cycles();

        let duration_cycles = ended_at.saturating_sub(self.start_cycles);

        critical_section::with(|cs| {
            let mut state = TRACE.borrow(cs).borrow_mut();

            if state.generation != self.generation {
                return;
            }

            state.depth = self.depth;

            let record = TraceRecord::new(
                self.callsite,
                self.depth,
                self.start_cycles,
                duration_cycles,
                self.values,
            );

            let active = state.active_buffer;

            state.buffers[active].push_span(record);
        });
    }
}

pub struct MetricTimer {
    generation: u32,

    callsite: &'static MetricCallsite,

    clock: Option<Clock>,
    start_cycles: u64,
}

impl MetricTimer {
    pub const fn disabled() -> Self {
        Self {
            generation: 0,

            callsite: &EMPTY_METRIC_CALLSITE,

            clock: None,
            start_cycles: 0,
        }
    }

    #[inline(always)]
    pub(crate) fn start(callsite: &'static MetricCallsite) -> Self {
        if !is_recording() {
            return Self::disabled();
        }

        critical_section::with(|cs| {
            let state = TRACE.borrow(cs).borrow();

            let Some(clock) = state.clock else {
                return Self::disabled();
            };

            Self {
                generation: state.generation,

                callsite,

                clock: Some(clock),

                start_cycles: clock.now_cycles(),
            }
        })
    }
}

impl Drop for MetricTimer {
    #[inline(always)]
    fn drop(&mut self) {
        let Some(clock) = self.clock else {
            return;
        };

        let ended_at = clock.now_cycles();

        let duration = ended_at.saturating_sub(self.start_cycles);

        let duration = u32::try_from(duration).unwrap_or(u32::MAX);

        observe_metric_for_generation(self.generation, self.callsite, duration);
    }
}

#[cfg(test)]
fn reset_for_test() {
    set_recording_enabled(false);

    critical_section::with(|cs| {
        *TRACE.borrow(cs).borrow_mut() = TraceState::new();
    });
}

#[cfg(test)]
mod tests {
    use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

    use std::sync::Mutex as StdMutex;

    use crate::{MetricKind, capture, init};

    static TEST_LOCK: StdMutex<()> = StdMutex::new(());
    static CYCLE_CLOCK: AtomicU32 = AtomicU32::new(0);
    static MONOTONIC_CLOCK: AtomicU64 = AtomicU64::new(0);
    static FIELD_EVALUATIONS: AtomicU32 = AtomicU32::new(0);

    fn test_lock() -> std::sync::MutexGuard<'static, ()> {
        TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn cycle_clock() -> u32 {
        CYCLE_CLOCK.load(Ordering::Relaxed)
    }

    fn monotonic_clock() -> u64 {
        MONOTONIC_CLOCK.load(Ordering::Relaxed)
    }

    fn evaluated_field() -> u32 {
        FIELD_EVALUATIONS.fetch_add(1, Ordering::Relaxed);

        7
    }

    fn reset() {
        super::reset_for_test();

        CYCLE_CLOCK.store(0, Ordering::Relaxed);
        MONOTONIC_CLOCK.store(0, Ordering::Relaxed);
        FIELD_EVALUATIONS.store(0, Ordering::Relaxed);
    }

    fn init_trace() {
        init(cycle_clock, 240_000_000, monotonic_clock, 1_000_000);
    }

    #[test]
    fn records_nested_spans_continuously() {
        let _test = test_lock();

        reset();

        CYCLE_CLOCK.store(100, Ordering::Relaxed);

        init_trace();

        CYCLE_CLOCK.store(110, Ordering::Relaxed);

        let outer = crate::span!(
            target: "ui.render",
            "render",
            frame = 42u32,
        );

        CYCLE_CLOCK.store(120, Ordering::Relaxed);

        let inner = crate::span!(
            target: "ui.text",
            "shape",
            bytes = 12usize,
            cached = true,
        );

        CYCLE_CLOCK.store(150, Ordering::Relaxed);

        drop(inner);

        CYCLE_CLOCK.store(170, Ordering::Relaxed);

        drop(outer);

        let capture = capture().unwrap();

        assert_eq!(capture.spans(), 2);

        assert_eq!(capture.overwritten(), 0);

        let inner = capture.span(0).unwrap();

        assert_eq!(inner.metadata().target(), "ui.text");

        assert_eq!(inner.start_cycles(), 20);

        assert_eq!(inner.duration_cycles(), 30);

        let outer = capture.span(1).unwrap();

        assert_eq!(outer.start_cycles(), 10);

        assert_eq!(outer.duration_cycles(), 60);
    }

    #[test]
    fn capture_does_not_stop_recording() {
        let _test = test_lock();

        reset();
        init_trace();

        CYCLE_CLOCK.store(10, Ordering::Relaxed);

        let first = crate::span!(
            target: "test",
            "first",
        );

        CYCLE_CLOCK.store(20, Ordering::Relaxed);

        drop(first);

        let first_capture = capture().unwrap();

        assert_eq!(first_capture.spans(), 1);

        CYCLE_CLOCK.store(30, Ordering::Relaxed);

        let second = crate::span!(
            target: "test",
            "second",
        );

        CYCLE_CLOCK.store(40, Ordering::Relaxed);

        drop(second);

        // The frozen transport buffer is
        // still owned by first_capture.
        assert!(capture().is_none());

        drop(first_capture);

        let second_capture = capture().unwrap();

        assert_eq!(second_capture.spans(), 1);

        assert_eq!(second_capture.span(0).unwrap().metadata().name(), "second");
    }

    #[test]
    fn span_can_cross_capture_boundary() {
        let _test = test_lock();

        reset();
        init_trace();

        CYCLE_CLOCK.store(10, Ordering::Relaxed);

        let span = crate::span!(
            target: "test",
            "cross_boundary",
        );

        CYCLE_CLOCK.store(20, Ordering::Relaxed);

        let first_capture = capture().unwrap();

        assert_eq!(first_capture.spans(), 0);

        CYCLE_CLOCK.store(40, Ordering::Relaxed);

        drop(span);

        drop(first_capture);

        let second_capture = capture().unwrap();

        let span = second_capture.span(0).unwrap();

        assert_eq!(span.start_cycles(), 10);

        assert_eq!(span.duration_cycles(), 30);
    }

    #[test]
    fn ring_keeps_most_recent_spans() {
        let _test = test_lock();

        reset();
        init_trace();

        for index in 0..=super::TRACE_CAPACITY {
            let start = u32::try_from(index * 2).unwrap();

            CYCLE_CLOCK.store(start, Ordering::Relaxed);

            let span = crate::span!(
                target: "test",
                "ring",
                index =
                    u32::try_from(
                        index,
                    )
                    .unwrap(),
            );

            CYCLE_CLOCK.store(start + 1, Ordering::Relaxed);

            drop(span);
        }

        let capture = capture().unwrap();

        assert_eq!(capture.spans(), super::TRACE_CAPACITY);

        assert_eq!(capture.overwritten(), 1);

        let oldest = capture.span(0).unwrap();

        assert_eq!(oldest.field(0).unwrap().value().as_u32(), Some(1));
    }

    #[test]
    fn long_idle_across_multiple_cycle_wraps_is_preserved() {
        let _test = test_lock();

        reset();

        CYCLE_CLOCK.store(100, Ordering::Relaxed);

        MONOTONIC_CLOCK.store(1_000, Ordering::Relaxed);

        init(cycle_clock, 1_000_000, monotonic_clock, 1_000_000);

        let elapsed = 2 * (1u64 << 32) + 50;

        MONOTONIC_CLOCK.store(1_000 + elapsed, Ordering::Relaxed);

        CYCLE_CLOCK.store(150, Ordering::Relaxed);

        let span = crate::span!(
            target: "test",
            "after_idle",
        );

        MONOTONIC_CLOCK.store(1_000 + elapsed + 10, Ordering::Relaxed);

        CYCLE_CLOCK.store(160, Ordering::Relaxed);

        drop(span);

        let capture = capture().unwrap();

        let span = capture.span(0).unwrap();

        assert_eq!(span.start_cycles(), elapsed);

        assert_eq!(span.duration_cycles(), 10);
    }

    #[test]
    fn aggregates_metrics_per_capture_chunk() {
        let _test = test_lock();

        reset();
        init_trace();

        fn cache_hit(value: u32) {
            crate::counter!(
                target:
                    "reader.pagination",
                "cache_hits",
                value,
            );
        }

        cache_hit(1);
        cache_hit(1);
        cache_hit(3);

        let capture = capture().unwrap();

        assert_eq!(capture.metrics(), 1);

        let metric = capture.metric(0).unwrap();

        assert_eq!(metric.kind(), MetricKind::Counter);

        assert_eq!(metric.count(), 3);

        assert_eq!(metric.sum(), 5);

        assert_eq!(metric.max(), 3);
    }

    #[test]
    fn metric_timer_survives_capture_boundary() {
        let _test = test_lock();

        reset();
        init_trace();

        CYCLE_CLOCK.store(10, Ordering::Relaxed);

        let timer = crate::timer!(
            target:
                "reader.pagination",
            "measure_text",
        );

        CYCLE_CLOCK.store(20, Ordering::Relaxed);

        let first = capture().unwrap();

        assert_eq!(first.metrics(), 0);

        CYCLE_CLOCK.store(50, Ordering::Relaxed);

        drop(timer);
        drop(first);

        let second = capture().unwrap();

        let metric = second.metric(0).unwrap();

        assert_eq!(metric.count(), 1);

        assert_eq!(metric.sum(), 40);
    }

    #[test]
    fn values_are_not_evaluated_before_init() {
        let _test = test_lock();

        reset();

        let span = crate::span!(
            target: "test",
            "disabled",
            value =
                evaluated_field(),
        );

        drop(span);

        crate::counter!(
            target: "test",
            "disabled_counter",
            evaluated_field(),
        );

        assert_eq!(FIELD_EVALUATIONS.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn callsite_definitions_are_local_to_capture() {
        let _test = test_lock();

        reset();
        init_trace();

        fn repeated() {
            let span = crate::span!(
                target:
                    "test.repeated",
                "repeat",
                value = 7u32,
            );

            drop(span);
        }

        repeated();
        repeated();

        let capture = capture().unwrap();

        assert!(capture.definition_at(0).is_some());

        assert!(capture.definition_at(1).is_none());

        assert_eq!(
            capture.span(0).unwrap().callsite_id(),
            capture.span(1).unwrap().callsite_id(),
        );
    }
}
