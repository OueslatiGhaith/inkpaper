#![no_std]

#[cfg(test)]
extern crate std;

#[cfg(feature = "recording")]
mod recording;

#[cfg(feature = "recording")]
pub use recording::{
    TRACE_CAPACITY, TraceRecord, TraceSession, TraceSpan, TraceSummary, record, set_clock,
};

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceEvent {
    Render = 0,
    Rebuild = 1,
    Layout = 2,
    Clear = 3,
    Paint = 4,
    Damage = 5,
    TextRun = 6,
    Shape = 7,
    Glyphs = 8,
    LogicalShape = 9,
    VisualOrder = 10,
    Positioning = 11,
}

impl TraceEvent {
    pub const ALL: [Self; 12] = [
        Self::Render,
        Self::Rebuild,
        Self::Layout,
        Self::Clear,
        Self::Paint,
        Self::Damage,
        Self::TextRun,
        Self::Shape,
        Self::Glyphs,
        Self::LogicalShape,
        Self::VisualOrder,
        Self::Positioning,
    ];

    pub const fn id(self) -> u8 {
        self as u8
    }

    pub const fn from_id(id: u8) -> Option<Self> {
        match id {
            0 => Some(Self::Render),
            1 => Some(Self::Rebuild),
            2 => Some(Self::Layout),
            3 => Some(Self::Clear),
            4 => Some(Self::Paint),
            5 => Some(Self::Damage),
            6 => Some(Self::TextRun),
            7 => Some(Self::Shape),
            8 => Some(Self::Glyphs),
            9 => Some(Self::LogicalShape),
            10 => Some(Self::VisualOrder),
            11 => Some(Self::Positioning),
            _ => None,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Render => "render",
            Self::Rebuild => "rebuild",
            Self::Layout => "layout",
            Self::Clear => "clear",
            Self::Paint => "paint",
            Self::Damage => "damage",
            Self::TextRun => "text_run",
            Self::Shape => "shape",
            Self::Glyphs => "glyphs",
            Self::LogicalShape => "logical_shape",
            Self::VisualOrder => "visual_order",
            Self::Positioning => "positioning",
        }
    }
}
