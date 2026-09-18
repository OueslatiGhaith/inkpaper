use core::cell::RefCell;

use critical_section::Mutex;

use crate::TraceEvent;

pub const TRACE_CAPACITY: usize = 256;

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
pub struct TraceSummary {
    session_id: u32,
    records: usize,
    dropped: u32,
    open_spans: u8,
}

impl TraceSummary {
    const EMPTY: Self = Self {
        session_id: 0,
        records: 0,
        dropped: 0,
        open_spans: 0,
    };

    pub const fn session_id(self) -> u32 {
        self.session_id
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

struct TraceState {
    clock: Option<ClockFn>,
    enabled: bool,
    session_id: u32,
    origin: u32,
    depth: u8,
    len: usize,
    dropped: u32,
    records: [TraceRecord; TRACE_CAPACITY],
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
}
