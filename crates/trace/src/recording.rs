use core::cell::RefCell;

use critical_section::Mutex;

use crate::{TraceEvent, TraceMetric};

pub const TRACE_CAPACITY: usize = 256;
pub const TRACE_ASYNC_CAPACITY: usize = 16;
pub const TRACE_ASYNC_OPEN_CAPACITY: usize = 4;

type ClockFn = fn() -> u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TraceRecord {
    event: TraceEvent,
    depth: u8,
    start_cycles: u32,
    duration_cycles: u32,
    arg: u32,
}

impl TraceRecord {
    const EMPTY: Self = Self {
        event: TraceEvent::Render,
        depth: 0,
        start_cycles: 0,
        duration_cycles: 0,
        arg: 0,
    };

    const fn new(
        event: TraceEvent,
        depth: u8,
        start_cycles: u32,
        duration_cycles: u32,
        arg: u32,
    ) -> Self {
        Self {
            event,
            depth,
            start_cycles,
            duration_cycles,
            arg,
        }
    }

    pub const fn event(self) -> TraceEvent {
        self.event
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

    pub const fn arg(self) -> u32 {
        self.arg
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TraceAsyncRecord {
    event: TraceEvent,
    id: u32,
    start_cycles: u32,
    duration_cycles: u32,
    arg: u32,
}

impl TraceAsyncRecord {
    const EMPTY: Self = Self {
        event: TraceEvent::Render,
        id: 0,
        start_cycles: 0,
        duration_cycles: 0,
        arg: 0,
    };

    pub const fn event(self) -> TraceEvent {
        self.event
    }

    pub const fn id(self) -> u32 {
        self.id
    }

    pub const fn start_cycles(self) -> u32 {
        self.start_cycles
    }

    pub const fn duration_cycles(self) -> u32 {
        self.duration_cycles
    }

    pub const fn arg(self) -> u32 {
        self.arg
    }
}

#[derive(Debug, Clone, Copy)]
struct TraceAsyncOpen {
    active: bool,
    event: TraceEvent,
    id: u32,
    start_cycles: u32,
    arg: u32,
}

impl TraceAsyncOpen {
    const EMPTY: Self = Self {
        active: false,
        event: TraceEvent::Render,
        id: 0,
        start_cycles: 0,
        arg: 0,
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TraceSummary {
    session_id: u32,
    origin_cycles: u32,
    records: usize,
    dropped: u32,
    open_spans: u8,
}

impl TraceSummary {
    const EMPTY: Self = Self {
        session_id: 0,
        origin_cycles: 0,
        records: 0,
        dropped: 0,
        open_spans: 0,
    };

    pub const fn session_id(self) -> u32 {
        self.session_id
    }

    pub const fn origin_cycles(self) -> u32 {
        self.origin_cycles
    }

    pub const fn records(self) -> usize {
        self.records
    }

    pub const fn dropped(self) -> u32 {
        self.dropped
    }

    pub const fn open_spans(self) -> u8 {
        self.open_spans
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TraceMetricTotals {
    calls: u32,
    cycles: u64,
}

impl TraceMetricTotals {
    const EMPTY: Self = Self {
        calls: 0,
        cycles: 0,
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TraceMetricRecord {
    metric: TraceMetric,
    calls: u32,
    cycles: u64,
}

impl TraceMetricRecord {
    pub const fn metric(self) -> TraceMetric {
        self.metric
    }

    pub const fn calls(self) -> u32 {
        self.calls
    }

    pub const fn cycles(self) -> u64 {
        self.cycles
    }
}

struct TraceState {
    clock: Option<ClockFn>,

    enabled: bool,
    session_id: u32,
    origin: u32,
    depth: u8,
    len: usize,
    dropped: u32,
    records: [TraceRecord; TRACE_CAPACITY],
    metrics: [TraceMetricTotals; TraceMetric::COUNT],

    async_len: usize,
    async_dropped: u32,
    async_records: [TraceAsyncRecord; TRACE_ASYNC_CAPACITY],
    async_open: [TraceAsyncOpen; TRACE_ASYNC_OPEN_CAPACITY],
}

impl TraceState {
    const fn new() -> Self {
        Self {
            clock: None,
            enabled: false,
            session_id: 0,
            origin: 0,
            depth: 0,
            len: 0,
            dropped: 0,
            records: [TraceRecord::EMPTY; TRACE_CAPACITY],
            metrics: [TraceMetricTotals::EMPTY; TraceMetric::COUNT],
            async_len: 0,
            async_dropped: 0,
            async_records: [TraceAsyncRecord::EMPTY; TRACE_ASYNC_CAPACITY],
            async_open: [TraceAsyncOpen::EMPTY; TRACE_ASYNC_OPEN_CAPACITY],
        }
    }
}

static TRACE: Mutex<RefCell<TraceState>> = Mutex::new(RefCell::new(TraceState::new()));

pub fn set_clock(clock: ClockFn) {
    critical_section::with(|cs| {
        TRACE.borrow(cs).borrow_mut().clock = Some(clock);
    });
}

pub struct TraceSession {
    session_id: u32,
    active: bool,
}

impl TraceSession {
    pub fn start() -> Self {
        critical_section::with(|cs| {
            let mut state = TRACE.borrow(cs).borrow_mut();

            let Some(clock) = state.clock else {
                return Self {
                    session_id: 0,
                    active: false,
                };
            };

            state.session_id = state.session_id.wrapping_add(1);
            state.origin = clock();
            state.depth = 0;
            state.len = 0;
            state.dropped = 0;
            state.enabled = true;
            state.metrics = [TraceMetricTotals::EMPTY; TraceMetric::COUNT];

            Self {
                session_id: state.session_id,
                active: true,
            }
        })
    }

    pub fn finish(mut self) -> TraceSummary {
        if !self.active {
            return TraceSummary::EMPTY;
        }

        self.active = false;

        finish_session(self.session_id)
    }
}

impl Drop for TraceSession {
    fn drop(&mut self) {
        if self.active {
            let _ = finish_session(self.session_id);
            self.active = false;
        }
    }
}

fn finish_session(session_id: u32) -> TraceSummary {
    critical_section::with(|cs| {
        let mut state = TRACE.borrow(cs).borrow_mut();

        if !state.enabled || state.session_id != session_id {
            return TraceSummary::EMPTY;
        }

        state.enabled = false;

        let summary = TraceSummary {
            session_id,
            origin_cycles: state.origin,
            records: state.len,
            dropped: state.dropped,
            open_spans: state.depth,
        };

        state.depth = 0;

        summary
    })
}

pub struct TraceSpan {
    session_id: u32,
    event: TraceEvent,
    depth: u8,
    start_cycles: u32,
    arg: u32,
    active: bool,
}

impl TraceSpan {
    #[inline(always)]
    pub fn start(event: TraceEvent) -> Self {
        Self::start_with_arg(event, 0)
    }

    #[inline(always)]
    pub fn start_with_arg(event: TraceEvent, arg: u32) -> Self {
        critical_section::with(|cs| {
            let mut state = TRACE.borrow(cs).borrow_mut();

            if !state.enabled {
                return Self::inactive(event);
            }

            let Some(clock) = state.clock else {
                return Self::inactive(event);
            };

            let start_cycles = clock().wrapping_sub(state.origin);
            let depth = state.depth;

            state.depth = state.depth.saturating_add(1);

            Self {
                session_id: state.session_id,
                event,
                depth,
                start_cycles,
                arg,
                active: true,
            }
        })
    }

    const fn inactive(event: TraceEvent) -> Self {
        Self {
            session_id: 0,
            event,
            depth: 0,
            start_cycles: 0,
            arg: 0,
            active: false,
        }
    }
}

impl Drop for TraceSpan {
    #[inline(always)]
    fn drop(&mut self) {
        if !self.active {
            return;
        }

        critical_section::with(|cs| {
            let mut state = TRACE.borrow(cs).borrow_mut();

            if !state.enabled || state.session_id != self.session_id {
                return;
            }

            let Some(clock) = state.clock else {
                return;
            };

            let end_cycles = clock().wrapping_sub(state.origin);
            let duration_cycles = end_cycles.wrapping_sub(self.start_cycles);

            // well-nested synchronous spans restore the depth they had on entry.
            state.depth = self.depth;

            let record = TraceRecord::new(
                self.event,
                self.depth,
                self.start_cycles,
                duration_cycles,
                self.arg,
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

pub struct TraceMetricTimer {
    session_id: u32,
    metric: TraceMetric,
    clock: Option<ClockFn>,
    started_at: u32,
}

impl TraceMetricTimer {
    #[inline(always)]
    pub fn start(metric: TraceMetric) -> Self {
        critical_section::with(|cs| {
            let state = TRACE.borrow(cs).borrow();

            if !state.enabled {
                return Self::inactive(metric);
            }

            let Some(clock) = state.clock else {
                return Self::inactive(metric);
            };

            Self {
                session_id: state.session_id,
                metric,
                clock: Some(clock),
                started_at: clock(),
            }
        })
    }

    const fn inactive(metric: TraceMetric) -> Self {
        Self {
            session_id: 0,
            metric,
            clock: None,
            started_at: 0,
        }
    }
}

impl Drop for TraceMetricTimer {
    #[inline(always)]
    fn drop(&mut self) {
        let Some(clock) = self.clock else {
            return;
        };

        // Take the ending timestamp before entering the recorder's critical section so
        // recorder bookkeeping is not counted as measured work.
        let elapsed = clock().wrapping_sub(self.started_at);

        critical_section::with(|cs| {
            let mut state = TRACE.borrow(cs).borrow_mut();

            if !state.enabled || state.session_id != self.session_id {
                return;
            }

            let index = usize::from(self.metric.id());

            let Some(metric) = state.metrics.get_mut(index) else {
                return;
            };

            metric.calls = metric.calls.saturating_add(1);
            metric.cycles = metric.cycles.saturating_add(u64::from(elapsed));
        });
    }
}

pub fn metric_record(metric: TraceMetric) -> TraceMetricRecord {
    critical_section::with(|cs| {
        let state = TRACE.borrow(cs).borrow();
        let totals = state.metrics[usize::from(metric.id())];

        TraceMetricRecord {
            metric,
            calls: totals.calls,
            cycles: totals.cycles,
        }
    })
}

pub fn record(index: usize) -> Option<TraceRecord> {
    critical_section::with(|cs| {
        let state = TRACE.borrow(cs).borrow();

        if index >= state.len {
            None
        } else {
            Some(state.records[index])
        }
    })
}

pub fn async_begin(event: TraceEvent, id: u32, arg: u32) -> bool {
    critical_section::with(|cs| {
        let mut state = TRACE.borrow(cs).borrow_mut();

        let Some(clock) = state.clock else {
            return false;
        };

        if state
            .async_open
            .iter()
            .any(|entry| entry.active && entry.event == event && entry.id == id)
        {
            return false;
        }

        let Some(index) = state.async_open.iter().position(|entry| !entry.active) else {
            state.async_dropped = state.async_dropped.saturating_add(1);

            return false;
        };

        state.async_open[index] = TraceAsyncOpen {
            active: true,
            event,
            id,
            start_cycles: clock(),
            arg,
        };

        true
    })
}

pub fn async_end(event: TraceEvent, id: u32) -> bool {
    critical_section::with(|cs| {
        let mut state = TRACE.borrow(cs).borrow_mut();

        let Some(index) = state
            .async_open
            .iter()
            .position(|entry| entry.active && entry.event == event && entry.id == id)
        else {
            return false;
        };

        let Some(clock) = state.clock else {
            return false;
        };

        let open = state.async_open[index];

        state.async_open[index] = TraceAsyncOpen::EMPTY;

        let record = TraceAsyncRecord {
            event: open.event,
            id: open.id,
            start_cycles: open.start_cycles,
            duration_cycles: clock().wrapping_sub(open.start_cycles),
            arg: open.arg,
        };

        if state.async_len >= TRACE_ASYNC_CAPACITY {
            state.async_dropped = state.async_dropped.saturating_add(1);

            return true;
        }

        let index = state.async_len;

        state.async_records[index] = record;
        state.async_len = index + 1;

        true
    })
}

pub fn async_record_count() -> usize {
    critical_section::with(|cs| TRACE.borrow(cs).borrow().async_len)
}

pub fn async_record(index: usize) -> Option<TraceAsyncRecord> {
    critical_section::with(|cs| {
        let state = TRACE.borrow(cs).borrow();

        if index >= state.async_len {
            None
        } else {
            Some(state.async_records[index])
        }
    })
}

pub fn async_dropped() -> u32 {
    critical_section::with(|cs| TRACE.borrow(cs).borrow().async_dropped)
}

pub fn async_open_count() -> u8 {
    critical_section::with(|cs| {
        let state = TRACE.borrow(cs).borrow();

        let count = state.async_open.iter().filter(|entry| entry.active).count();

        u8::try_from(count).unwrap_or(u8::MAX)
    })
}

pub fn clear_async_records() {
    critical_section::with(|cs| {
        let mut state = TRACE.borrow(cs).borrow_mut();

        state.async_len = 0;
        state.async_dropped = 0;
    });
}

#[cfg(test)]
mod tests {
    use core::sync::atomic::{AtomicU32, Ordering};

    use crate::{
        TraceEvent,
        recording::{TraceSession, TraceSpan, record, set_clock},
    };

    static CLOCK: AtomicU32 = AtomicU32::new(0);

    fn test_clock() -> u32 {
        CLOCK.load(Ordering::Relaxed)
    }

    #[test]
    fn records_nested_spans_relative_to_session_origin() {
        set_clock(test_clock);

        CLOCK.store(100, Ordering::Relaxed);
        let session = TraceSession::start();

        CLOCK.store(110, Ordering::Relaxed);
        let outer = TraceSpan::start_with_arg(TraceEvent::Render, 42);

        CLOCK.store(120, Ordering::Relaxed);
        let inner = TraceSpan::start(TraceEvent::Shape);

        CLOCK.store(150, Ordering::Relaxed);
        drop(inner);

        CLOCK.store(170, Ordering::Relaxed);
        drop(outer);

        let summary = session.finish();

        assert_eq!(summary.records(), 2);
        assert_eq!(summary.dropped(), 0);
        assert_eq!(summary.open_spans(), 0);
        assert_eq!(summary.origin_cycles(), 100);

        let inner = record(0).unwrap();

        assert_eq!(inner.event(), TraceEvent::Shape);
        assert_eq!(inner.depth(), 1);
        assert_eq!(inner.start_cycles(), 20);
        assert_eq!(inner.duration_cycles(), 30);
        assert_eq!(inner.arg(), 0);

        let outer = record(1).unwrap();

        assert_eq!(outer.event(), TraceEvent::Render);
        assert_eq!(outer.depth(), 0);
        assert_eq!(outer.start_cycles(), 10);
        assert_eq!(outer.duration_cycles(), 60);
        assert_eq!(outer.arg(), 42);
    }

    #[test]
    fn profiling_macros_record_scopes_and_expressions() {
        set_clock(test_clock);

        CLOCK.store(100, Ordering::Relaxed);

        let session = TraceSession::start();

        CLOCK.store(110, Ordering::Relaxed);

        {
            crate::profile_scope!(TraceEvent::Paint, arg = 7usize,);

            CLOCK.store(140, Ordering::Relaxed);
        }

        CLOCK.store(150, Ordering::Relaxed);

        let value = crate::profile_expr!(TraceEvent::Damage, arg = 9usize, {
            CLOCK.store(180, Ordering::Relaxed);
            42u32
        },);

        assert_eq!(value, 42);

        let summary = session.finish();

        assert_eq!(summary.records(), 2);
        assert_eq!(summary.dropped(), 0);
        assert_eq!(summary.open_spans(), 0);

        let paint = record(0).unwrap();

        assert_eq!(paint.event(), TraceEvent::Paint);
        assert_eq!(paint.depth(), 0);
        assert_eq!(paint.start_cycles(), 10);
        assert_eq!(paint.duration_cycles(), 30);
        assert_eq!(paint.arg(), 7);

        let damage = record(1).unwrap();

        assert_eq!(damage.event(), TraceEvent::Damage);
        assert_eq!(damage.depth(), 0);
        assert_eq!(damage.start_cycles(), 50);
        assert_eq!(damage.duration_cycles(), 30);
        assert_eq!(damage.arg(), 9);
    }

    #[test]
    fn aggregates_hot_metrics_without_consuming_span_capacity() {
        use crate::{
            TraceMetric,
            recording::{TraceMetricTimer, metric_record},
        };

        set_clock(test_clock);

        CLOCK.store(100, Ordering::Relaxed);
        let session = TraceSession::start();

        CLOCK.store(110, Ordering::Relaxed);
        let first = TraceMetricTimer::start(TraceMetric::PairPositioning);

        CLOCK.store(140, Ordering::Relaxed);
        drop(first);

        CLOCK.store(150, Ordering::Relaxed);
        let second = TraceMetricTimer::start(TraceMetric::PairPositioning);

        CLOCK.store(175, Ordering::Relaxed);
        drop(second);

        let summary = session.finish();

        assert_eq!(summary.records(), 0);
        assert_eq!(summary.dropped(), 0);

        let metric = metric_record(TraceMetric::PairPositioning);

        assert_eq!(metric.metric(), TraceMetric::PairPositioning);
        assert_eq!(metric.calls(), 2);
        assert_eq!(metric.cycles(), 55);
    }
}
