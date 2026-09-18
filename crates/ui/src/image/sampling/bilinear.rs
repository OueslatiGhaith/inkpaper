use crate::{Color, ImageResource};

use super::axis::LinearQuotient;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BilinearAxis {
    first: u32,
    second: u32,
    fraction: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct BilinearRow {
    y_axis: BilinearAxis,
}

pub(super) struct BilinearCursor {
    position: LinearQuotient,
    source_width: u32,
    remaining: u32,
}

pub(super) fn prepare_row(
    y: u32,
    source_height: u32,
    destination_height: u32,
) -> Option<BilinearRow> {
    Some(BilinearRow {
        y_axis: bilinear_axis(y, source_height, destination_height)?,
    })
}

pub(super) fn prepare_cursor(
    start_x: u32,
    source_width: u32,
    destination_width: u32,
    _row: BilinearRow,
) -> Option<BilinearCursor> {
    if source_width == 0 || destination_width == 0 || start_x >= destination_width {
        return None;
    }

    // same coordinate progression as bilinear_axis():
    //
    // ((2x + 1) * source_width * 256)
    // --------------------------------
    //        2 * destination_width
    //
    // incrementing x adds 2 * source_width * 256 to the numerator.
    let initial_numerator = (u128::from(start_x) * 2 + 1) * u128::from(source_width) * 256;

    let step_numerator = u64::from(source_width).checked_mul(512)?;

    let denominator = u64::from(destination_width).checked_mul(2)?;

    Some(BilinearCursor {
        position: LinearQuotient::new(initial_numerator, step_numerator, denominator)?,
        source_width,
        remaining: destination_width - start_x,
    })
}

pub(super) fn sample_next(
    image: &dyn ImageResource,
    row: BilinearRow,
    cursor: &mut BilinearCursor,
) -> Option<Color> {
    if cursor.remaining == 0 {
        return None;
    }

    let x_axis = axis_from_position(cursor.position, cursor.source_width)?;

    cursor.remaining -= 1;

    if cursor.remaining > 0 {
        cursor.position.advance();
    }

    let y_axis = row.y_axis;

    let top_left = image.pixel(x_axis.first, y_axis.first)?;

    let top_right = image.pixel(x_axis.second, y_axis.first)?;

    let bottom_left = image.pixel(x_axis.first, y_axis.second)?;

    let bottom_right = image.pixel(x_axis.second, y_axis.second)?;

    let top = lerp_color(top_left, top_right, x_axis.fraction);

    let bottom = lerp_color(bottom_left, bottom_right, x_axis.fraction);

    Some(lerp_color(top, bottom, y_axis.fraction))
}

fn bilinear_axis(destination: u32, source_len: u32, destination_len: u32) -> Option<BilinearAxis> {
    if source_len == 0 || destination_len == 0 || destination >= destination_len {
        return None;
    }

    if source_len <= 1 {
        return Some(BilinearAxis {
            first: 0,
            second: 0,
            fraction: 0,
        });
    }

    let numerator = (i128::from(destination) * 2 + 1) * i128::from(source_len) * 256;
    let denominator = i128::from(destination_len) * 2;
    let coordinate = numerator / denominator - 128;
    let maximum = i128::from(source_len - 1) * 256;
    let coordinate = coordinate.clamp(0, maximum);

    axis_from_fixed_coordinate(u64::try_from(coordinate).ok()?, source_len)
}

fn axis_from_position(position: LinearQuotient, source_len: u32) -> Option<BilinearAxis> {
    let coordinate = position.quotient().saturating_sub(128);

    let maximum = u64::from(source_len.saturating_sub(1)) * 256;

    axis_from_fixed_coordinate(coordinate.min(maximum), source_len)
}

fn axis_from_fixed_coordinate(coordinate: u64, source_len: u32) -> Option<BilinearAxis> {
    if source_len == 0 {
        return None;
    }

    let first = u32::try_from(coordinate / 256).ok()?.min(source_len - 1);

    let second = first.saturating_add(1).min(source_len - 1);

    let fraction = u16::try_from(coordinate % 256).ok()?;

    Some(BilinearAxis {
        first,
        second,
        fraction,
    })
}

fn lerp_color(first: Color, second: Color, fraction: u16) -> Color {
    Color::rgb(
        lerp_channel(first.r(), second.r(), fraction),
        lerp_channel(first.g(), second.g(), fraction),
        lerp_channel(first.b(), second.b(), fraction),
    )
}

fn lerp_channel(first: u8, second: u8, fraction: u16) -> u8 {
    let fraction = u32::from(fraction);
    let inverse = 256 - fraction;
    let value = (u32::from(first) * inverse + u32::from(second) * fraction + 128) / 256;

    u8::try_from(value).unwrap_or(u8::MAX)
}
