use core::cmp::{max, min};

use num_traits::{PrimInt, Unsigned};

use crate::{Color, ImageResource};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AreaAxis {
    start: u64,
    end: u64,
    source_pixel_span: u64,
    first_source: u32,
    end_source: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct AreaRow {
    y_axis: AreaAxis,
    total_weight: u64,
}

pub(super) fn prepare_row(
    y: u32,
    source_width: u32,
    source_height: u32,
    destination_height: u32,
) -> Option<AreaRow> {
    let y_axis = area_axis(y, source_height, destination_height)?;

    let total_weight = u64::from(source_width) * u64::from(source_height);

    Some(AreaRow {
        y_axis,
        total_weight,
    })
}

pub(super) fn sample_area(
    image: &dyn ImageResource,
    x: u32,
    source_width: u32,
    destination_width: u32,
    row: AreaRow,
) -> Option<Color> {
    let x_axis = area_axis(x, source_width, destination_width)?;

    sample_area_with::<u64>(image, x_axis, row.y_axis, row.total_weight)
        .or_else(|| sample_area_with::<u128>(image, x_axis, row.y_axis, row.total_weight))
}

fn area_axis(destination: u32, source_len: u32, destination_len: u32) -> Option<AreaAxis> {
    if source_len == 0 || destination_len == 0 || destination >= destination_len {
        return None;
    }

    let source_len = u64::from(source_len);
    let destination_len_u64 = u64::from(destination_len);

    let start = u64::from(destination) * source_len;
    let end = (u64::from(destination) + 1) * source_len;

    let first_source = u32::try_from(start / destination_len_u64)
        .ok()?
        .min(u32::try_from(source_len.saturating_sub(1)).ok()?);

    let end_source = u32::try_from(ceil_div(end, destination_len_u64).min(source_len)).ok()?;

    Some(AreaAxis {
        start,
        end,
        source_pixel_span: destination_len_u64,
        first_source,
        end_source,
    })
}

fn sample_area_with<T>(
    image: &dyn ImageResource,
    x_axis: AreaAxis,
    y_axis: AreaAxis,
    total_weight: u64,
) -> Option<Color>
where
    T: PrimInt + Unsigned,
{
    let total_weight = T::from(total_weight)?;

    let mut red = T::zero();
    let mut green = T::zero();
    let mut blue = T::zero();

    for source_y in y_axis.first_source..y_axis.end_source {
        let source_top = u64::from(source_y) * y_axis.source_pixel_span;

        let source_bottom = (u64::from(source_y) + 1) * y_axis.source_pixel_span;

        let y_weight = overlap_length(y_axis.start, y_axis.end, source_top, source_bottom);

        if y_weight == 0 {
            continue;
        }

        let y_weight = T::from(y_weight)?;

        for source_x in x_axis.first_source..x_axis.end_source {
            let source_left = u64::from(source_x) * x_axis.source_pixel_span;

            let source_right = (u64::from(source_x) + 1) * x_axis.source_pixel_span;

            let x_weight = overlap_length(x_axis.start, x_axis.end, source_left, source_right);

            if x_weight == 0 {
                continue;
            }

            let color = image.pixel(source_x, source_y)?;

            let x_weight = T::from(x_weight)?;
            let weight = x_weight.checked_mul(&y_weight)?;

            red = accumulate_channel(red, color.r(), weight)?;
            green = accumulate_channel(green, color.g(), weight)?;
            blue = accumulate_channel(blue, color.b(), weight)?;
        }
    }

    Some(Color::rgb(
        weighted_channel(red, total_weight)?,
        weighted_channel(green, total_weight)?,
        weighted_channel(blue, total_weight)?,
    ))
}

fn ceil_div(value: u64, divisor: u64) -> u64 {
    debug_assert!(divisor > 0);
    value / divisor + u64::from(!value.is_multiple_of(divisor))
}

fn overlap_length(first_start: u64, first_end: u64, second_start: u64, second_end: u64) -> u64 {
    min(first_end, second_end).saturating_sub(max(first_start, second_start))
}

fn accumulate_channel<T>(accumulator: T, channel: u8, weight: T) -> Option<T>
where
    T: PrimInt + Unsigned,
{
    let channel = T::from(channel)?;
    let weighted = channel.checked_mul(&weight)?;

    accumulator.checked_add(&weighted)
}

fn weighted_channel<T>(weighted_sum: T, total_weight: T) -> Option<u8>
where
    T: PrimInt + Unsigned,
{
    debug_assert!(!total_weight.is_zero());

    let two = T::one() + T::one();

    let rounded = weighted_sum.checked_add(&(total_weight / two))? / total_weight;

    rounded.to_u8()
}
