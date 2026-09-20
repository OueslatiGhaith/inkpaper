#![no_std]

#[cfg(test)]
extern crate std;

#[cfg(feature = "recording")]
mod recording;
mod schema;

pub use inkpaper_trace_macros::profile;
pub use schema::{DisplayPhase, TraceEvent, TraceMetric};

#[cfg(feature = "recording")]
pub use recording::{
    TRACE_ASYNC_CAPACITY, TRACE_ASYNC_OPEN_CAPACITY, TRACE_CAPACITY, TraceAsyncRecord,
    TraceMetricRecord, TraceMetricTimer, TraceRecord, TraceSession, TraceSpan, TraceSummary,
    async_begin, async_dropped, async_end, async_open_count, async_record, async_record_count,
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
