#![no_std]

#[cfg(test)]
extern crate std;

mod metadata;

#[cfg(feature = "recording")]
mod recording;
#[cfg(feature = "recording")]
mod text;

pub use metadata::{
    Callsite, Field, IntoMetricValue, IntoValue, Metadata, MetricCallsite, MetricKind, Value,
    ValueKind,
};

#[cfg(feature = "recording")]
pub use recording::{
    CallsiteDefinition, CallsiteId, CaptureEntry, CapturedMetric, CapturedSpan, METRIC_CAPACITY,
    MetricId, MetricTimer, RecordedField, Span, TRACE_CAPACITY, TraceCapture, capture, init,
};
#[cfg(feature = "recording")]
pub use text::{TextEncodeError, write_text_capture};

#[cfg(not(feature = "recording"))]
#[derive(Debug, Clone, Copy, Default)]
pub struct Span;

#[cfg(not(feature = "recording"))]
impl Span {
    pub const fn disabled() -> Self {
        Self
    }
}

#[cfg(not(feature = "recording"))]
#[derive(Debug, Clone, Copy, Default)]
pub struct MetricTimer;

#[cfg(not(feature = "recording"))]
impl MetricTimer {
    pub const fn disabled() -> Self {
        Self
    }
}

#[doc(hidden)]
pub mod __private {
    #[inline(always)]
    pub fn into_value<T>(value: T) -> crate::Value
    where
        T: crate::IntoValue,
    {
        crate::metadata::into_value(value)
    }

    #[cfg(feature = "recording")]
    #[inline(always)]
    pub fn enabled() -> bool {
        crate::recording::is_recording()
    }

    #[cfg(feature = "recording")]
    #[inline(always)]
    pub fn start(callsite: &'static crate::Callsite, values: [crate::Value; 2]) -> crate::Span {
        crate::Span::start(callsite, values)
    }

    #[inline(always)]
    pub fn into_metric_value<T>(value: T) -> u32
    where
        T: crate::IntoMetricValue,
    {
        crate::metadata::into_metric_value(value)
    }

    #[cfg(feature = "recording")]
    #[inline(always)]
    pub fn observe_metric(callsite: &'static crate::MetricCallsite, value: u32) {
        crate::recording::observe_metric(callsite, value);
    }

    #[cfg(feature = "recording")]
    #[inline(always)]
    pub fn start_metric_timer(callsite: &'static crate::MetricCallsite) -> crate::MetricTimer {
        crate::MetricTimer::start(callsite)
    }
}

#[cfg(feature = "recording")]
#[macro_export]
macro_rules! span {
    (
        target: $target:expr,
        $name:literal,
        $field0:ident = $value0:expr,
        $field1:ident = $value1:expr,
        $field2:ident = $value2:expr
        $(, $rest:tt)*
    ) => {
        compile_error!(
            "inkpaper_trace::span! supports at most two fields"
        )
    };

    (
        target: $target:expr,
        $name:literal,
        $field0:ident = $value0:expr,
        $field1:ident = $value1:expr
        $(,)?
    ) => {{
        static __INKPAPER_TRACE_FIELDS: [$crate::Field; 2] = [
            $crate::Field::new(stringify!($field0)),
            $crate::Field::new(stringify!($field1)),
        ];

        static __INKPAPER_TRACE_CALLSITE: $crate::Callsite =
            $crate::Callsite::new(
                $name,
                $target,
                &__INKPAPER_TRACE_FIELDS,
            );

        if $crate::__private::enabled() {
            $crate::__private::start(
                &__INKPAPER_TRACE_CALLSITE,
                [
                    $crate::__private::into_value($value0),
                    $crate::__private::into_value($value1),
                ],
            )
        } else {
            $crate::Span::disabled()
        }
    }};

    (
        target: $target:expr,
        $name:literal,
        $field0:ident = $value0:expr
        $(,)?
    ) => {{
        static __INKPAPER_TRACE_FIELDS: [$crate::Field; 1] = [
            $crate::Field::new(stringify!($field0)),
        ];

        static __INKPAPER_TRACE_CALLSITE: $crate::Callsite =
            $crate::Callsite::new(
                $name,
                $target,
                &__INKPAPER_TRACE_FIELDS,
            );

        if $crate::__private::enabled() {
            $crate::__private::start(
                &__INKPAPER_TRACE_CALLSITE,
                [
                    $crate::__private::into_value($value0),
                    $crate::Value::EMPTY,
                ],
            )
        } else {
            $crate::Span::disabled()
        }
    }};

    (
        target: $target:expr,
        $name:literal
        $(,)?
    ) => {{
        static __INKPAPER_TRACE_FIELDS: [$crate::Field; 0] = [];

        static __INKPAPER_TRACE_CALLSITE: $crate::Callsite =
            $crate::Callsite::new(
                $name,
                $target,
                &__INKPAPER_TRACE_FIELDS,
            );

        if $crate::__private::enabled() {
            $crate::__private::start(
                &__INKPAPER_TRACE_CALLSITE,
                [$crate::Value::EMPTY; 2],
            )
        } else {
            $crate::Span::disabled()
        }
    }};

    (
        $name:literal,
        $field0:ident = $value0:expr,
        $field1:ident = $value1:expr
        $(,)?
    ) => {
        $crate::span!(
            target: module_path!(),
            $name,
            $field0 = $value0,
            $field1 = $value1,
        )
    };

    (
        $name:literal,
        $field0:ident = $value0:expr
        $(,)?
    ) => {
        $crate::span!(
            target: module_path!(),
            $name,
            $field0 = $value0,
        )
    };

    (
        $name:literal
        $(,)?
    ) => {
        $crate::span!(
            target: module_path!(),
            $name,
        )
    };
}

#[cfg(not(feature = "recording"))]
#[macro_export]
macro_rules! span {
    (
        target: $target:expr,
        $name:literal,
        $field0:ident = $value0:expr,
        $field1:ident = $value1:expr,
        $field2:ident = $value2:expr
        $(, $rest:tt)*
    ) => {
        compile_error!(
            "inkpaper_trace::span! supports at most two fields"
        )
    };

    (
        target: $target:expr,
        $name:literal
        $(, $field:ident = $value:expr)*
        $(,)?
    ) => {{
        if false {
            let _: &'static str = $target;
            let _: &'static str = $name;

            $(
                let _ = &$value;
            )*
        }

        $crate::Span::disabled()
    }};

    (
        $name:literal
        $(, $field:ident = $value:expr)*
        $(,)?
    ) => {
        $crate::span!(
            target: module_path!(),
            $name
            $(, $field = $value)*
        )
    };
}

#[cfg(feature = "recording")]
#[macro_export]
macro_rules! counter {
    (
        target: $target:expr,
        $name:literal,
        $value:expr
        $(,)?
    ) => {{
        static __INKPAPER_METRIC_CALLSITE:
            $crate::MetricCallsite =
            $crate::MetricCallsite::new(
                $name,
                $target,
                $crate::MetricKind::Counter,
                "",
            );

        if $crate::__private::enabled() {
            let value =
                $crate::__private::into_metric_value(
                    $value,
                );

            $crate::__private::observe_metric(
                &__INKPAPER_METRIC_CALLSITE,
                value,
            );
        }
    }};

    (
        $name:literal,
        $value:expr
        $(,)?
    ) => {
        $crate::counter!(
            target: module_path!(),
            $name,
            $value,
        )
    };
}

#[cfg(not(feature = "recording"))]
#[macro_export]
macro_rules! counter {
    (
        target: $target:expr,
        $name:literal,
        $value:expr
        $(,)?
    ) => {{
        if false {
            let _: &'static str =
                $target;
            let _: &'static str =
                $name;
            let _ = &$value;
        }
    }};

    (
        $name:literal,
        $value:expr
        $(,)?
    ) => {
        $crate::counter!(
            target: module_path!(),
            $name,
            $value,
        )
    };
}

#[cfg(feature = "recording")]
#[macro_export]
macro_rules! distribution {
    (
        target: $target:expr,
        $name:literal,
        $value:expr
        $(,)?
    ) => {{
        static __INKPAPER_METRIC_CALLSITE:
            $crate::MetricCallsite =
            $crate::MetricCallsite::new(
                $name,
                $target,
                $crate::MetricKind::Distribution,
                "",
            );

        if $crate::__private::enabled() {
            let value =
                $crate::__private::into_metric_value(
                    $value,
                );

            $crate::__private::observe_metric(
                &__INKPAPER_METRIC_CALLSITE,
                value,
            );
        }
    }};

    (
        $name:literal,
        $value:expr
        $(,)?
    ) => {
        $crate::distribution!(
            target: module_path!(),
            $name,
            $value,
        )
    };
}

#[cfg(not(feature = "recording"))]
#[macro_export]
macro_rules! distribution {
    (
        target: $target:expr,
        $name:literal,
        $value:expr
        $(,)?
    ) => {{
        if false {
            let _: &'static str =
                $target;
            let _: &'static str =
                $name;
            let _ = &$value;
        }
    }};

    (
        $name:literal,
        $value:expr
        $(,)?
    ) => {
        $crate::distribution!(
            target: module_path!(),
            $name,
            $value,
        )
    };
}

#[cfg(feature = "recording")]
#[macro_export]
macro_rules! timer {
    (
        target: $target:expr,
        $name:literal
        $(,)?
    ) => {{
        static __INKPAPER_METRIC_CALLSITE:
            $crate::MetricCallsite =
            $crate::MetricCallsite::new(
                $name,
                $target,
                $crate::MetricKind::Distribution,
                "cycles",
            );

        if $crate::__private::enabled() {
            $crate::__private::start_metric_timer(
                &__INKPAPER_METRIC_CALLSITE,
            )
        } else {
            $crate::MetricTimer::disabled()
        }
    }};

    (
        $name:literal
        $(,)?
    ) => {
        $crate::timer!(
            target: module_path!(),
            $name,
        )
    };
}

#[cfg(not(feature = "recording"))]
#[macro_export]
macro_rules! timer {
    (
        target: $target:expr,
        $name:literal
        $(,)?
    ) => {{
        if false {
            let _: &'static str =
                $target;
            let _: &'static str =
                $name;
        }

        $crate::MetricTimer::disabled()
    }};

    (
        $name:literal
        $(,)?
    ) => {
        $crate::timer!(
            target: module_path!(),
            $name,
        )
    };
}
