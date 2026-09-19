#![no_std]

#[cfg(test)]
extern crate std;

#[cfg(feature = "recording")]
mod recording;

pub use inkpaper_trace_macros::profile;

#[cfg(feature = "recording")]
pub use recording::{
    TRACE_CAPACITY, TraceRecord, TraceSession, TraceSpan, TraceSummary, record, set_clock,
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

macro_rules! define_trace_events {
    (
        $( $variant:ident = $id:literal => $name:literal ),+ $(,)?
    ) => {
        #[repr(u8)]
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum TraceEvent {
            $( $variant = $id, )+
        }

        impl TraceEvent {
            pub const ALL: &'static [Self] = &[
                $(
                    Self::$variant,
                )+
            ];

            pub const fn id(self) -> u8 {
                self as u8
            }

            pub const fn from_id(
                id: u8,
            ) -> Option<Self> {
                match id {
                    $( $id => Some(Self::$variant), )+
                    _ => None,
                }
            }

            pub const fn name(
                self,
            ) -> &'static str {
                match self {
                    $( Self::$variant => $name, )+
                }
            }
        }
    };
}

define_trace_events! {
    Render = 0 => "render",
    Rebuild = 1 => "rebuild",
    Layout = 2 => "layout",
    Clear = 3 => "clear",
    Paint = 4 => "paint",
    Damage = 5 => "damage",

    TextRun = 6 => "text_run",
    Shape = 7 => "shape",
    Glyphs = 8 => "glyphs",
    LogicalShape = 9 => "logical_shape",
    VisualOrder = 10 => "visual_order",
    Positioning = 11 => "positioning",

    Present = 12 => "present",
    PresentBusy = 13 => "present_busy",
}
