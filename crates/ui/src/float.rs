//! float to integer rounding for pixel and font maths. `as` saturates at the
//! integer's bounds and turns NaN into 0, which is what these callers want, so the
//! casts the cast lints flag live here once

#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

pub(crate) fn floor_i32(value: f32) -> i32 {
    libm::floorf(value) as i32
}

pub(crate) fn ceil_i32(value: f32) -> i32 {
    libm::ceilf(value) as i32
}

#[cfg(feature = "ttf")]
pub(crate) fn round_i32(value: f32) -> i32 {
    libm::roundf(value) as i32
}

/// negative values become 0
pub(crate) fn ceil_usize(value: f32) -> usize {
    libm::ceilf(value) as usize
}
