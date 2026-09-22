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

type ClockFn = fn() -> u32;

static EMPTY_FIELDS: [Field; 0] = [];
static EMPTY_CALLSITE: Callsite = Callsite::new("<empty>", "trace", &EMPTY_FIELDS);
static EMPTY_METRIC_CALLSITE: MetricCallsite =
    MetricCallsite::new("<empty>", "trace", MetricKind::Counter, "");

#[cfg(target_has_atomic = "32")]
static RECORDING_ENABLED: AtomicU32 = AtomicU32::new(0);

#[cfg(not(target_has_atomic = "32"))]
static RECORDING_ENABLED: Mutex<RefCell<bool>> = Mutex::new(RefCell::new(false));

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
    callsite: &'static Callsite,

    start_cycles: u32,
    duration_cycles: u32,

    value0: u32,
    value1: u32,

    depth: u8,
    value_kinds: u8,
}

#[cfg(target_pointer_width = "32")]
const _: () = {
    assert!(
        core::mem::size_of::<TraceRecord>() <= 24,
        "TraceRecord exceeded the 24-byte 32-bit target budget",
    );
};

impl TraceRecord {
    const EMPTY: Self = Self {
        callsite: &EMPTY_CALLSITE,

        start_cycles: 0,
        duration_cycles: 0,

        value0: 0,
        value1: 0,

        depth: 0,
        value_kinds: 0,
    };

    fn new(
        callsite: &'static Callsite,
        depth: u8,
        start_cycles: u32,
        duration_cycles: u32,
        values: [Value; 2],
    ) -> Self {
        let value_kinds = (values[0].kind() as u8) | ((values[1].kind() as u8) << 2);

        Self {
            callsite,

            start_cycles,
            duration_cycles,

            value0: values[0].raw(),
            value1: values[1].raw(),

            depth,
            value_kinds,
        }
    }

    pub const fn callsite(self) -> &'static Callsite {
        self.callsite
    }

    pub const fn metadata(self) -> &'static crate::Metadata {
        self.callsite.metadata()
    }

    pub const fn depth(self) -> u8 {
        self.depth
    }

    pub const fn start_cycles(self) -> u32 {
        self.start_cycles
    }

    pub const fn duration_cycles(self) -> u32 {
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
        self.record.callsite.metadata()
    }

    pub const fn depth(self) -> u8 {
        self.record.depth
    }

    pub const fn start_cycles(self) -> u32 {
        self.record.start_cycles
    }

    pub const fn duration_cycles(self) -> u32 {
        self.record.duration_cycles
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TraceCapture {
    session_id: u32,
    clock_hz: u32,
    origin_cycles: u32,

    spans: usize,
    dropped: u32,
    open_spans: u8,

    metrics: usize,
    metric_dropped: u32,
}

impl TraceCapture {
    const EMPTY: Self = Self {
        session_id: 0,
        clock_hz: 0,
        origin_cycles: 0,

        spans: 0,
        dropped: 0,
        open_spans: 0,

        metrics: 0,
        metric_dropped: 0,
    };

    pub const fn session_id(self) -> u32 {
        self.session_id
    }

    pub const fn clock_hz(self) -> u32 {
        self.clock_hz
    }

    pub const fn origin_cycles(self) -> u32 {
        self.origin_cycles
    }

    pub const fn spans(self) -> usize {
        self.spans
    }

    pub const fn dropped(self) -> u32 {
        self.dropped
    }

    pub const fn open_spans(self) -> u8 {
        self.open_spans
    }

    pub const fn metrics(self) -> usize {
        self.metrics
    }

    pub const fn metric_dropped(self) -> u32 {
        self.metric_dropped
    }

    pub fn entry(self, index: usize) -> Option<CaptureEntry> {
        critical_section::with(|cs| {
            let state = TRACE.borrow(cs).borrow();

            if !self.matches(&state) || index >= self.spans {
                return None;
            }

            let record = *state.records.get(index)?;

            let callsite_id = callsite_id_for(&state.records[..self.spans], index)?;

            let definition = if usize::from(callsite_id.get()) == index {
                Some(CallsiteDefinition {
                    id: callsite_id,
                    metadata: record.callsite.metadata(),
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

    pub fn span(self, index: usize) -> Option<CapturedSpan> {
        self.entry(index).map(CaptureEntry::span)
    }

    pub fn definition_at(self, index: usize) -> Option<CallsiteDefinition> {
        self.entry(index).and_then(CaptureEntry::definition)
    }

    pub fn metric(self, index: usize) -> Option<CapturedMetric> {
        critical_section::with(|cs| {
            let state = TRACE.borrow(cs).borrow();

            if !self.matches(&state) || index >= self.metrics {
                return None;
            }

            let record = *state.metrics.get(index)?;

            let id = MetricId(u16::try_from(index).ok()?);

            Some(CapturedMetric { id, record })
        })
    }

    fn matches(self, state: &TraceState) -> bool {
        self.session_id != 0
            && !state.active
            && state.session_id == self.session_id
            && state.len == self.spans
            && state.metric_len == self.metrics
    }
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

fn callsite_id_for(records: &[TraceRecord], index: usize) -> Option<CallsiteId> {
    let record = records.get(index)?;

    let first_index = records[..=index]
        .iter()
        .position(|candidate| core::ptr::eq(candidate.callsite, record.callsite))?;

    Some(CallsiteId(u16::try_from(first_index).ok()?))
}

struct TraceState {
    clock: Option<ClockFn>,
    clock_hz: u32,

    active: bool,
    session_id: u32,
    origin_cycles: u32,

    depth: u8,

    len: usize,
    dropped: u32,
    records: [TraceRecord; TRACE_CAPACITY],

    metric_len: usize,
    metric_dropped: u32,
    metrics: [MetricRecord; METRIC_CAPACITY],
}

impl TraceState {
    const fn new() -> Self {
        Self {
            clock: None,
            clock_hz: 0,

            active: false,
            session_id: 0,
            origin_cycles: 0,

            depth: 0,

            len: 0,
            dropped: 0,
            records: [TraceRecord::EMPTY; TRACE_CAPACITY],

            metric_len: 0,
            metric_dropped: 0,
            metrics: [MetricRecord::EMPTY; METRIC_CAPACITY],
        }
    }
}

static TRACE: Mutex<RefCell<TraceState>> = Mutex::new(RefCell::new(TraceState::new()));

pub fn set_clock(clock: ClockFn, clock_hz: u32) {
    assert!(clock_hz > 0, "trace clock frequency must be non-zero");

    critical_section::with(|cs| {
        let mut state = TRACE.borrow(cs).borrow_mut();

        state.clock = Some(clock);
        state.clock_hz = clock_hz;
    });
}

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

#[inline(always)]
pub(crate) fn observe_metric(callsite: &'static MetricCallsite, value: u32) {
    record_metric(callsite, value, None);
}

fn observe_metric_for_session(session_id: u32, callsite: &'static MetricCallsite, value: u32) {
    record_metric(callsite, value, Some(session_id));
}

fn record_metric(callsite: &'static MetricCallsite, value: u32, expected_session: Option<u32>) {
    if !is_recording() {
        return;
    }

    critical_section::with(|cs| {
        let mut state = TRACE.borrow(cs).borrow_mut();

        if !state.active {
            return;
        }

        if let Some(session_id) = expected_session
            && state.session_id != session_id
        {
            return;
        }

        let existing = state.metrics[..state.metric_len]
            .iter()
            .position(|metric| core::ptr::eq(metric.callsite, callsite));

        let index = if let Some(index) = existing {
            index
        } else {
            if state.metric_len >= METRIC_CAPACITY {
                state.metric_dropped = state.metric_dropped.saturating_add(1);

                return;
            }

            let index = state.metric_len;

            state.metrics[index] = MetricRecord::new(callsite);

            state.metric_len = index + 1;

            index
        };

        state.metrics[index].observe(value);
    });
}

pub struct TraceSession {
    session_id: u32,
    active: bool,
}

impl TraceSession {
    pub fn start() -> Self {
        let session_id = critical_section::with(|cs| {
            let mut state = TRACE.borrow(cs).borrow_mut();

            if state.active {
                return 0;
            }

            let Some(clock) = state.clock else {
                return 0;
            };

            if state.clock_hz == 0 {
                return 0;
            }

            let mut session_id = state.session_id.wrapping_add(1);

            if session_id == 0 {
                session_id = 1;
            }

            state.active = true;
            state.session_id = session_id;
            state.origin_cycles = clock();

            state.depth = 0;

            state.len = 0;
            state.dropped = 0;

            state.metric_len = 0;
            state.metric_dropped = 0;

            session_id
        });

        let active = session_id != 0;

        if active {
            set_recording_enabled(true);
        }

        Self { session_id, active }
    }

    pub fn finish(mut self) -> TraceCapture {
        let capture = self.finish_inner();

        self.active = false;

        capture
    }

    fn finish_inner(&self) -> TraceCapture {
        if !self.active {
            return TraceCapture::EMPTY;
        }

        let capture = critical_section::with(|cs| {
            let mut state = TRACE.borrow(cs).borrow_mut();

            if !state.active || state.session_id != self.session_id {
                return TraceCapture::EMPTY;
            }

            state.active = false;

            let capture = TraceCapture {
                session_id: state.session_id,
                clock_hz: state.clock_hz,
                origin_cycles: state.origin_cycles,

                spans: state.len,
                dropped: state.dropped,
                open_spans: state.depth,

                metrics: state.metric_len,
                metric_dropped: state.metric_dropped,
            };

            state.depth = 0;

            capture
        });

        if capture.session_id != 0 {
            set_recording_enabled(false);
        }

        capture
    }
}

impl Drop for TraceSession {
    fn drop(&mut self) {
        let _ = self.finish_inner();

        self.active = false;
    }
}

pub struct Span {
    session_id: u32,

    callsite: &'static Callsite,

    clock: Option<ClockFn>,
    start_cycles: u32,

    values: [Value; 2],

    depth: u8,
}

impl Span {
    pub const fn disabled() -> Self {
        Self {
            session_id: 0,

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

            if !state.active {
                return Self::disabled();
            }

            let Some(clock) = state.clock else {
                return Self::disabled();
            };

            let depth = state.depth;

            state.depth = state.depth.saturating_add(1);

            Self {
                session_id: state.session_id,

                callsite,

                clock: Some(clock),
                start_cycles: clock().wrapping_sub(state.origin_cycles),

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

        // Read the ending timestamp before entering the recorder critical
        // section. Recorder bookkeeping is not part of the measured span.
        let ended_at = clock();

        critical_section::with(|cs| {
            let mut state = TRACE.borrow(cs).borrow_mut();

            if !state.active || state.session_id != self.session_id {
                return;
            }

            let end_cycles = ended_at.wrapping_sub(state.origin_cycles);

            let duration_cycles = end_cycles.wrapping_sub(self.start_cycles);

            state.depth = self.depth;

            let record = TraceRecord::new(
                self.callsite,
                self.depth,
                self.start_cycles,
                duration_cycles,
                self.values,
            );

            if state.len < TRACE_CAPACITY {
                let index = state.len;

                state.records[index] = record;
                state.len = index + 1;
            } else {
                state.dropped = state.dropped.saturating_add(1);
            }
        });
    }
}

pub struct MetricTimer {
    session_id: u32,
    callsite: &'static MetricCallsite,
    clock: Option<ClockFn>,
    start_cycles: u32,
}

impl MetricTimer {
    pub const fn disabled() -> Self {
        Self {
            session_id: 0,
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

            if !state.active {
                return Self::disabled();
            }

            let Some(clock) = state.clock else {
                return Self::disabled();
            };

            Self {
                session_id: state.session_id,
                callsite,
                clock: Some(clock),
                start_cycles: clock(),
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

        let end_cycles = clock();

        let duration_cycles = end_cycles.wrapping_sub(self.start_cycles);

        observe_metric_for_session(self.session_id, self.callsite, duration_cycles);
    }
}

#[cfg(test)]
mod tests {
    use core::sync::atomic::{AtomicU32, Ordering};

    use std::sync::Mutex as StdMutex;

    use crate::{MetricKind, TraceSession, set_clock};

    static TEST_LOCK: StdMutex<()> = StdMutex::new(());

    static CLOCK: AtomicU32 = AtomicU32::new(0);
    static FIELD_EVALUATIONS: AtomicU32 = AtomicU32::new(0);

    fn test_lock() -> std::sync::MutexGuard<'static, ()> {
        TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn test_clock() -> u32 {
        CLOCK.load(Ordering::Relaxed)
    }

    fn evaluated_field() -> u32 {
        FIELD_EVALUATIONS.fetch_add(1, Ordering::Relaxed);

        7
    }

    #[test]
    fn records_nested_spans_with_static_metadata_and_fields() {
        let _test = test_lock();

        set_clock(test_clock, 240_000_000);

        CLOCK.store(100, Ordering::Relaxed);

        let session = TraceSession::start();

        CLOCK.store(110, Ordering::Relaxed);

        let outer = crate::span!(target: "ui.render", "render", frame = 42u32);

        CLOCK.store(120, Ordering::Relaxed);

        let inner = crate::span!(target: "ui.text", "shape", bytes = 12usize, cached = true);

        CLOCK.store(150, Ordering::Relaxed);
        drop(inner);

        CLOCK.store(170, Ordering::Relaxed);
        drop(outer);

        let capture = session.finish();

        assert_eq!(capture.spans(), 2);
        assert_eq!(capture.dropped(), 0);
        assert_eq!(capture.open_spans(), 0);
        assert_eq!(capture.origin_cycles(), 100);

        let inner = capture.span(0).unwrap();

        assert_eq!(inner.metadata().target(), "ui.text");
        assert_eq!(inner.metadata().name(), "shape");

        assert_eq!(inner.depth(), 1);
        assert_eq!(inner.start_cycles(), 20);
        assert_eq!(inner.duration_cycles(), 30);

        let bytes = inner.field(0).unwrap();

        assert_eq!(bytes.name(), "bytes");
        assert_eq!(bytes.value().as_u32(), Some(12));

        let cached = inner.field(1).unwrap();

        assert_eq!(cached.name(), "cached");
        assert_eq!(cached.value().as_bool(), Some(true));

        assert!(inner.field(2).is_none());

        let outer = capture.span(1).unwrap();

        assert_eq!(outer.metadata().target(), "ui.render");
        assert_eq!(outer.metadata().name(), "render");

        assert_eq!(outer.depth(), 0);
        assert_eq!(outer.start_cycles(), 10);
        assert_eq!(outer.duration_cycles(), 60);

        let frame = outer.field(0).unwrap();

        assert_eq!(frame.name(), "frame");
        assert_eq!(frame.value().as_u32(), Some(42));

        assert!(outer.field(1).is_none());
    }

    #[test]
    fn fields_are_not_evaluated_without_an_active_session() {
        let _test = test_lock();

        set_clock(test_clock, 240_000_000);

        FIELD_EVALUATIONS.store(0, Ordering::Relaxed);

        let span = crate::span!(target: "test", "disabled", value = evaluated_field());

        drop(span);

        assert_eq!(FIELD_EVALUATIONS.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn reports_overflow_without_overwriting_existing_records() {
        let _test = test_lock();

        set_clock(test_clock, 240_000_000);

        CLOCK.store(0, Ordering::Relaxed);

        let session = TraceSession::start();

        for tick in 0..=super::TRACE_CAPACITY {
            CLOCK.store(u32::try_from(tick).unwrap(), Ordering::Relaxed);

            let span = crate::span!(target: "test", "overflow");

            CLOCK.store(u32::try_from(tick + 1).unwrap(), Ordering::Relaxed);

            drop(span);
        }

        let capture = session.finish();

        assert_eq!(capture.spans(), super::TRACE_CAPACITY);

        assert_eq!(capture.dropped(), 1);

        assert!(capture.span(super::TRACE_CAPACITY - 1).is_some());

        assert!(capture.span(super::TRACE_CAPACITY).is_none());
    }

    #[test]
    fn cycle_wrap_preserves_relative_start_and_duration() {
        let _test = test_lock();

        set_clock(test_clock, 240_000_000);

        CLOCK.store(u32::MAX - 20, Ordering::Relaxed);

        let session = TraceSession::start();

        CLOCK.store(u32::MAX - 10, Ordering::Relaxed);

        let span = crate::span!(target: "test", "wrap");

        CLOCK.store(15, Ordering::Relaxed);

        drop(span);

        let capture = session.finish();
        let span = capture.span(0).unwrap();

        assert_eq!(capture.origin_cycles(), u32::MAX - 20);

        assert_eq!(span.start_cycles(), 10);
        assert_eq!(span.duration_cycles(), 26);
    }

    #[test]
    fn nested_trace_sessions_are_rejected() {
        let _test = test_lock();

        set_clock(test_clock, 240_000_000);

        CLOCK.store(100, Ordering::Relaxed);

        let outer = TraceSession::start();
        let inner = TraceSession::start();

        CLOCK.store(110, Ordering::Relaxed);

        let span = crate::span!(
            target: "test",
            "outer_session",
        );

        CLOCK.store(120, Ordering::Relaxed);

        drop(span);

        let inner_capture = inner.finish();
        let outer_capture = outer.finish();

        assert_eq!(inner_capture.session_id(), 0);
        assert_eq!(inner_capture.spans(), 0);

        assert_ne!(outer_capture.session_id(), 0);
        assert_eq!(outer_capture.spans(), 1);
    }

    #[test]
    fn host_trace_record_stays_within_the_experimental_budget() {
        let _test = test_lock();

        assert!(core::mem::size_of::<super::TraceRecord>() <= 32);
    }

    #[test]
    fn callsite_definitions_are_emitted_once_per_capture() {
        let _test = test_lock();

        set_clock(test_clock, 240_000_000);

        CLOCK.store(100, Ordering::Relaxed);

        let session = TraceSession::start();

        fn repeated_span() {
            let span = crate::span!(
                target: "test.repeated",
                "repeat",
                value = 7u32,
            );

            drop(span);
        }

        CLOCK.store(110, Ordering::Relaxed);
        repeated_span();

        CLOCK.store(120, Ordering::Relaxed);
        repeated_span();

        CLOCK.store(130, Ordering::Relaxed);

        let unique = crate::span!(
            target: "test.unique",
            "unique",
        );

        CLOCK.store(140, Ordering::Relaxed);
        drop(unique);

        let capture = session.finish();

        assert_eq!(capture.spans(), 3);

        let repeated_definition = capture.definition_at(0).unwrap();

        assert_eq!(repeated_definition.id().get(), 0);

        assert_eq!(repeated_definition.metadata().target(), "test.repeated");

        assert_eq!(repeated_definition.metadata().name(), "repeat");

        // Span 1 uses the same static callsite as span 0.
        assert!(capture.definition_at(1).is_none());

        let unique_definition = capture.definition_at(2).unwrap();

        assert_eq!(unique_definition.id().get(), 2);

        assert_eq!(unique_definition.metadata().target(), "test.unique");

        assert_eq!(unique_definition.metadata().name(), "unique");

        assert_eq!(
            capture.span(0).unwrap().callsite_id(),
            capture.span(1).unwrap().callsite_id(),
        );

        assert_ne!(
            capture.span(0).unwrap().callsite_id(),
            capture.span(2).unwrap().callsite_id(),
        );
    }

    #[test]
    fn capture_becomes_stale_when_a_new_session_starts() {
        let _test = test_lock();

        set_clock(test_clock, 240_000_000);

        CLOCK.store(100, Ordering::Relaxed);

        let first_session = TraceSession::start();

        let first_span = crate::span!(target: "test", "first");

        drop(first_span);

        let first_capture = first_session.finish();

        assert!(first_capture.span(0).is_some());

        CLOCK.store(200, Ordering::Relaxed);

        let second_session = TraceSession::start();

        assert!(first_capture.span(0).is_none());
        assert!(first_capture.definition_at(0).is_none());

        let _ = second_session.finish();
    }

    #[test]
    fn text_capture_is_self_describing() {
        let _test = test_lock();

        set_clock(test_clock, 240_000_000);

        CLOCK.store(100, Ordering::Relaxed);

        let session = TraceSession::start();

        fn repeated_span() {
            let span = crate::span!(
                target: "reader.pagination",
                "measure",
                bytes = 12u32,
                cached = true,
            );

            drop(span);
        }

        CLOCK.store(110, Ordering::Relaxed);
        repeated_span();

        CLOCK.store(120, Ordering::Relaxed);
        repeated_span();

        CLOCK.store(130, Ordering::Relaxed);

        let layout = crate::span!(
            target: "ui.layout",
            "resolve",
        );

        CLOCK.store(145, Ordering::Relaxed);
        drop(layout);

        let capture = session.finish();

        let mut output = std::string::String::new();

        crate::write_text_capture(capture, &mut output).unwrap();

        let expected = std::format!(
            concat!(
                "trace/v2 capture session={} hz=240000000 origin=100 spans=3 dropped=0 open=0\n",
                "trace/v2 define id=0 target=17:reader.pagination name=7:measure fields=2 5:bytes 6:cached\n",
                "trace/v2 span id=0 depth=0 start=10 cycles=0 values=2 u:12 b:1\n",
                "trace/v2 span id=0 depth=0 start=20 cycles=0 values=2 u:12 b:1\n",
                "trace/v2 define id=2 target=9:ui.layout name=7:resolve fields=0\n",
                "trace/v2 span id=2 depth=0 start=30 cycles=15 values=0\n",
                "trace/v2 end session={}\n",
            ),
            capture.session_id(),
            capture.session_id(),
        );

        assert_eq!(output, expected);
    }

    #[test]
    fn text_capture_preserves_field_types() {
        let _test = test_lock();

        set_clock(test_clock, 240_000_000);

        CLOCK.store(100, Ordering::Relaxed);

        let session = TraceSession::start();

        let span = crate::span!(
            target: "test",
            "values",
            delta = -17i32,
            ready = false,
        );

        CLOCK.store(120, Ordering::Relaxed);

        drop(span);

        let capture = session.finish();

        let mut output = std::string::String::new();

        crate::write_text_capture(capture, &mut output).unwrap();

        assert!(output.contains("values=2 i:-17 b:0"));
    }

    #[test]
    fn aggregates_metrics_by_static_callsite() {
        let _test = test_lock();

        set_clock(test_clock, 240_000_000);

        CLOCK.store(100, Ordering::Relaxed);

        let session = TraceSession::start();

        fn cache_hit(value: u32) {
            crate::counter!(
                target: "reader.pagination",
                "cache_hits",
                value,
            );
        }

        fn measured_bytes(value: u32) {
            crate::distribution!(
                target: "reader.pagination",
                "word_bytes",
                value,
            );
        }

        cache_hit(1);
        cache_hit(1);
        cache_hit(3);

        measured_bytes(12);
        measured_bytes(7);
        measured_bytes(21);

        let capture = session.finish();

        assert_eq!(capture.metrics(), 2);
        assert_eq!(capture.metric_dropped(), 0);

        let counter = capture.metric(0).unwrap();

        assert_eq!(counter.metadata().target(), "reader.pagination");

        assert_eq!(counter.metadata().name(), "cache_hits");

        assert_eq!(counter.kind(), MetricKind::Counter);

        assert_eq!(counter.unit(), "");
        assert_eq!(counter.count(), 3);
        assert_eq!(counter.sum(), 5);
        assert_eq!(counter.max(), 3);

        let distribution = capture.metric(1).unwrap();

        assert_eq!(distribution.metadata().name(), "word_bytes");

        assert_eq!(distribution.kind(), MetricKind::Distribution);

        assert_eq!(distribution.count(), 3);

        assert_eq!(distribution.sum(), 40);

        assert_eq!(distribution.max(), 21);

        assert!(capture.metric(2).is_none());
    }

    #[test]
    fn timer_records_cycle_distribution() {
        let _test = test_lock();

        set_clock(test_clock, 240_000_000);

        CLOCK.store(100, Ordering::Relaxed);

        let session = TraceSession::start();

        CLOCK.store(110, Ordering::Relaxed);

        let timer = crate::timer!(
            target: "reader.pagination",
            "measure_text",
        );

        CLOCK.store(170, Ordering::Relaxed);

        drop(timer);

        let capture = session.finish();

        assert_eq!(capture.metrics(), 1);

        let metric = capture.metric(0).unwrap();

        assert_eq!(metric.kind(), MetricKind::Distribution);

        assert_eq!(metric.unit(), "cycles");

        assert_eq!(metric.count(), 1);
        assert_eq!(metric.sum(), 60);
        assert_eq!(metric.max(), 60);
    }

    #[test]
    fn timer_handles_cycle_counter_wrap() {
        let _test = test_lock();

        set_clock(test_clock, 240_000_000);

        CLOCK.store(u32::MAX - 20, Ordering::Relaxed);

        let session = TraceSession::start();

        CLOCK.store(u32::MAX - 10, Ordering::Relaxed);

        let timer = crate::timer!(
            target: "test",
            "wrapped_timer",
        );

        CLOCK.store(15, Ordering::Relaxed);

        drop(timer);

        let capture = session.finish();

        let metric = capture.metric(0).unwrap();

        assert_eq!(metric.count(), 1);
        assert_eq!(metric.sum(), 26);
        assert_eq!(metric.max(), 26);
    }

    #[test]
    fn metric_values_are_not_evaluated_without_session() {
        let _test = test_lock();

        set_clock(test_clock, 240_000_000);

        FIELD_EVALUATIONS.store(0, Ordering::Relaxed);

        crate::counter!(
            target: "test",
            "disabled_counter",
            evaluated_field(),
        );

        crate::distribution!(
            target: "test",
            "disabled_distribution",
            evaluated_field(),
        );

        assert_eq!(FIELD_EVALUATIONS.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn text_capture_writes_metric_summaries() {
        let _test = test_lock();

        set_clock(test_clock, 240_000_000);

        CLOCK.store(100, Ordering::Relaxed);

        let session = TraceSession::start();

        fn cache_hit(value: u32) {
            crate::counter!(
                target: "reader.pagination",
                "cache_hits",
                value,
            );
        }

        fn word_bytes(value: u32) {
            crate::distribution!(
                target: "reader.pagination",
                "word_bytes",
                value,
            );
        }

        cache_hit(1);
        cache_hit(1);

        word_bytes(12);
        word_bytes(7);

        let capture = session.finish();

        let mut output = std::string::String::new();

        crate::write_text_capture(capture, &mut output).unwrap();

        assert!(output.contains("trace/v2 metrics count=2 dropped=0"));

        assert!(output.contains(concat!(
            "trace/v2 metric_define id=0 ",
            "target=17:reader.pagination ",
            "name=10:cache_hits ",
            "kind=counter unit=0:",
        )));

        assert!(output.contains("trace/v2 metric id=0 count=2 sum=2 max=1"));

        assert!(output.contains(concat!(
            "trace/v2 metric_define id=1 ",
            "target=17:reader.pagination ",
            "name=10:word_bytes ",
            "kind=distribution unit=0:",
        )));

        assert!(output.contains("trace/v2 metric id=1 count=2 sum=19 max=12"));
    }
}
