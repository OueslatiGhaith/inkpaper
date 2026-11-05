//! Reopening at boot the book read when the device went to sleep, like
//! crosspoint, within the time Settings allows.

use alloc::{string::String, vec::Vec};

/// How long after going to sleep the book read then opens again at boot.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResumeLimit {
    #[default]
    Always,
    Hour,
    HalfHour,
    TenMinutes,
    Never,
}

impl ResumeLimit {
    pub(crate) const ALL: [Self; 5] = [
        Self::Always,
        Self::Hour,
        Self::HalfHour,
        Self::TenMinutes,
        Self::Never,
    ];

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Always => "Always",
            Self::Hour => "1 hour",
            Self::HalfHour => "30 min",
            Self::TenMinutes => "10 min",
            Self::Never => "Never",
        }
    }

    /// The picker's options, in [`ResumeLimit::ALL`]'s order.
    pub(crate) fn options() -> Vec<String> {
        Self::ALL
            .iter()
            .map(|limit| String::from(limit.label()))
            .collect()
    }

    /// The stored form, and the option's index in the picker.
    pub(crate) const fn index(self) -> u8 {
        match self {
            Self::Always => 0,
            Self::Hour => 1,
            Self::HalfHour => 2,
            Self::TenMinutes => 3,
            Self::Never => 4,
        }
    }

    pub(crate) const fn from_index(index: u8) -> Option<Self> {
        match index {
            0 => Some(Self::Always),
            1 => Some(Self::Hour),
            2 => Some(Self::HalfHour),
            3 => Some(Self::TenMinutes),
            4 => Some(Self::Never),
            _ => None,
        }
    }

    /// Whether the book read when the device went to sleep `slept_at` opens
    /// again `now`, both in minutes since 1970. Without either time, or when
    /// the clock went back, it opens: a clock that isn't set never keeps the
    /// book closed.
    pub(crate) fn allows(self, slept_at: Option<u32>, now: Option<u32>) -> bool {
        let limit = match self {
            Self::Always => return true,
            Self::Never => return false,
            Self::Hour => 60,
            Self::HalfHour => 30,
            Self::TenMinutes => 10,
        };

        match (slept_at, now) {
            (Some(slept_at), Some(now)) => {
                now.checked_sub(slept_at).is_none_or(|slept| slept <= limit)
            }
            _ => true,
        }
    }
}

/// The book read when the device went to sleep, and when it did, in minutes
/// since 1970 when the clock was set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SleptBook {
    pub(crate) path: String,
    pub(crate) slept_at: Option<u32>,
}
