use crate::{Color, ImageResource};

pub(super) fn sample_bilinear(
    image: &dyn ImageResource,
    x: u32,
    y: u32,
    source_width: u32,
    source_height: u32,
    destination_width: u32,
    destination_height: u32,
) -> Option<Color> {
    let (x0, x1, x_fraction) = bilinear_axis(x, source_width, destination_width);

    let (y0, y1, y_fraction) = bilinear_axis(y, source_height, destination_height);

    let top_left = image.pixel(x0, y0)?;
    let top_right = image.pixel(x1, y0)?;
    let bottom_left = image.pixel(x0, y1)?;
    let bottom_right = image.pixel(x1, y1)?;

    let top = lerp_color(top_left, top_right, x_fraction);
    let bottom = lerp_color(bottom_left, bottom_right, x_fraction);

    Some(lerp_color(top, bottom, y_fraction))
}

fn bilinear_axis(destination: u32, source_len: u32, destination_len: u32) -> (u32, u32, u16) {
    if source_len <= 1 {
        return (0, 0, 0);
    }

    debug_assert!(destination_len > 0);

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

    (first, second, fraction)
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
