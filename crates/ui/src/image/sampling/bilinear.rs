use crate::{Color, ImageResource};

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

pub(super) fn prepare_row(
    y: u32,
    source_height: u32,
    destination_height: u32,
) -> Option<BilinearRow> {
    Some(BilinearRow {
        y_axis: bilinear_axis(y, source_height, destination_height)?,
    })
}

pub(super) fn sample_bilinear(
    image: &dyn ImageResource,
    x: u32,
    source_width: u32,
    destination_width: u32,
    row: BilinearRow,
) -> Option<Color> {
    let x_axis = bilinear_axis(x, source_width, destination_width)?;

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

    // map destination pixel centers to source pixel centers using 8 bits
    // of fractional precision:
    // source = ((destination + 0.5) * source_len / destination_len) - 0.5
    let numerator = (i128::from(destination) * 2 + 1) * i128::from(source_len) * 256;
    let denominator = i128::from(destination_len) * 2;
    let coordinate = numerator / denominator - 128;
    let maximum = i128::from(source_len - 1) * 256;
    let coordinate = coordinate.clamp(0, maximum);
    let first = u32::try_from(coordinate / 256).unwrap_or(source_len - 1);
    let second = first.saturating_add(1).min(source_len - 1);
    let fraction = u16::try_from(coordinate % 256).unwrap_or(0);

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
