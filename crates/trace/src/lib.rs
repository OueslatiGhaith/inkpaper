#![no_std]

#[cfg(test)]
extern crate std;

#[cfg(feature = "recording")]
mod recording;
mod schema;

pub use inkpaper_trace_macros::profile;
pub use schema::{DisplayPhase, TraceAggregate, TraceEvent, TraceMetric};

#[cfg(feature = "recording")]
pub use recording::{
    TRACE_ASYNC_CAPACITY, TRACE_ASYNC_OPEN_CAPACITY, TRACE_CAPACITY, TraceAggregateRecord,
    TraceAggregates, TraceAsyncRecord, TraceMetricRecord, TraceMetricTimer, TraceRecord,
    TraceSession, TraceSpan, TraceSummary, aggregate_record, async_begin, async_dropped, async_end,
    async_open_count, async_record, async_record_count, clear_aggregate_records,
    clear_async_records, metric_record, record, set_clock,
};

#[cfg(feature = "recording")]
#[doc(hidden)]
#[inline(always)]
pub fn __trace_arg<T>(value: T) -> u32
where
    u32: TryFrom<T>,
{
    u32::try_from(value).unwrap_or(u32::MAX)
}

#[cfg(feature = "recording")]
#[doc(hidden)]
#[inline(always)]
pub fn __trace_value<T>(value: T) -> u64
where
    u64: TryFrom<T>,
{
    u64::try_from(value).unwrap_or(u64::MAX)
}

#[cfg(not(feature = "recording"))]
#[derive(Debug, Default)]
pub struct TraceAggregates;

#[cfg(not(feature = "recording"))]
impl TraceAggregates {
    pub const fn new() -> Self {
        Self
    }
}

#[cfg(feature = "recording")]
#[macro_export]
macro_rules! profile_span {
    ($event:expr $(,)?) => {
        $crate::TraceSpan::start($event)
    };

    ($event:expr, arg = $arg:expr $(,)?) => {
        $crate::TraceSpan::start_with_arg($event, $crate::__trace_arg($arg))
    };
}

#[cfg(not(feature = "recording"))]
#[macro_export]
macro_rules! profile_span {
    ($event:expr $(,)?) => {{
        if false {
            let _ = $event;
        }

        ()
    }};

    ($event:expr, arg = $arg:expr $(,)?) => {{
        if false {
            let _ = $event;
        }

        ()
    }};
}

#[cfg(feature = "recording")]
#[macro_export]
macro_rules! profile_scope {
    ($event:expr $(,)?) => {
        let _inkpaper_trace_span = $crate::profile_span!($event);
    };

    ($event:expr, arg = $arg:expr $(,)?) => {
        let _inkpaper_trace_span = $crate::profile_span!($event, arg = $arg);
    };
}

#[cfg(not(feature = "recording"))]
#[macro_export]
macro_rules! profile_scope {
    ($event:expr $(,)?) => {
        if false {
            let _ = $event;
        }
    };

    ($event:expr, arg = $arg:expr $(,)?) => {
        if false {
            let _ = $event;
        }
    };
}

#[cfg(feature = "recording")]
#[macro_export]
macro_rules! profile_expr {
    ($event:expr, $expression:expr $(,)?) => {{
        let _inkpaper_trace_span = $crate::profile_span!($event);
        $expression
    }};

    ($event:expr, arg = $arg:expr, $expression:expr $(,)?) => {{
        let _inkpaper_trace_span = $crate::profile_span!($event, arg = $arg);
        $expression
    }};
}

#[cfg(not(feature = "recording"))]
#[macro_export]
macro_rules! profile_expr {
    ($event:expr, arg = $arg:expr, $expression:expr $(,)?) => {{
        if false {
            let _ = $event;
        }

        $expression
    }};

    ($event:expr, $expression:expr $(,)?) => {{
        if false {
            let _ = $event;
        }

        $expression
    }};
}

#[cfg(feature = "recording")]
#[macro_export]
macro_rules! profile_metric_scope {
    ($metric:expr $(,)?) => {
        let _inkpaper_trace_metric_timer = $crate::TraceMetricTimer::start($metric);
    };
}

#[cfg(not(feature = "recording"))]
#[macro_export]
macro_rules! profile_metric_scope {
    ($metric:expr $(,)?) => {
        if false {
            let _ = $metric;
        }
    };
}

#[cfg(feature = "recording")]
#[macro_export]
macro_rules! profile_metric_expr {
    ($metric:expr, $expression:expr $(,)?) => {{
        let _inkpaper_trace_metric_timer = $crate::TraceMetricTimer::start($metric);
        $expression
    }};
}

#[cfg(not(feature = "recording"))]
#[macro_export]
macro_rules! profile_metric_expr {
    ($metric:expr, $expression:expr $(,)?) => {{
        if false {
            let _ = $metric;
        }

        $expression
    }};
}

#[cfg(feature = "recording")]
#[macro_export]
macro_rules! profile_aggregate_expr {
    (
        $aggregates:expr,
        $aggregate:expr,
        $expression:expr
        $(,)?
    ) => {{
        let __inkpaper_trace_started = $aggregates.__start();

        let __inkpaper_trace_result = $expression;

        $aggregates.__finish($aggregate, __inkpaper_trace_started);

        __inkpaper_trace_result
    }};
}

#[cfg(not(feature = "recording"))]
#[macro_export]
macro_rules! profile_aggregate_expr {
    (
        $aggregates:expr,
        $aggregate:expr,
        $expression:expr
        $(,)?
    ) => {{
        if false {
            let _ = &$aggregates;
            let _ = &$aggregate;
        }

        $expression
    }};
}

#[cfg(feature = "recording")]
#[macro_export]
macro_rules! trace_aggregate {
    ($aggregates:expr, $aggregate:expr $(,)?) => {{
        $aggregates.__observe($aggregate, 1);
    }};

    (
        $aggregates:expr,
        $aggregate:expr,
        $value:expr
        $(,)?
    ) => {{
        let __inkpaper_trace_value = $crate::__trace_value($value);

        $aggregates.__observe($aggregate, __inkpaper_trace_value);
    }};
}

#[cfg(not(feature = "recording"))]
#[macro_export]
macro_rules! trace_aggregate {
    ($aggregates:expr, $aggregate:expr $(,)?) => {{
        if false {
            let _ = &$aggregates;
            let _ = &$aggregate;
        }
    }};

    (
        $aggregates:expr,
        $aggregate:expr,
        $value:expr
        $(,)?
    ) => {{
        if false {
            let _ = &$aggregates;
            let _ = &$aggregate;
            let _ = &$value;
        }
    }};
}
