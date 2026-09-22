#![no_std]

#[cfg(test)]
extern crate std;

mod metadata;

#[cfg(feature = "recording")]
mod recording;
#[cfg(feature = "recording")]
mod text;

pub use metadata::{Callsite, Field, IntoValue, Metadata, Value, ValueKind};

#[cfg(feature = "recording")]
pub use recording::{
    CallsiteDefinition, CallsiteId, CaptureEntry, CapturedSpan, RecordedField, Span,
    TRACE_CAPACITY, TraceCapture, TraceSession, set_clock,
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
