use core::cmp::{max, min};

use crate::{Color, ImageResource, ImageSampling};

#[allow(clippy::too_many_arguments)]
pub(crate) fn sample_image(
    image: &dyn ImageResource,
    x: u32,
    y: u32,
    source_width: u32,
    source_height: u32,
    destination_width: u32,
    destination_height: u32,
    sampling: ImageSampling,
) -> Option<Color> {
    if source_width == 0
        || source_height == 0
        || destination_width == 0
        || destination_height == 0
        || x >= destination_width
        || y >= destination_height
    {
        return None;
    }

    match sampling {
        ImageSampling::Nearest => sample_nearest(
            image,
            x,
            y,
            source_width,
            source_height,
            destination_width,
            destination_height,
        ),
        ImageSampling::Bilinear => sample_bilinear(
            image,
            x,
            y,
            source_width,
            source_height,
            destination_width,
            destination_height,
        ),
        ImageSampling::Area => {
            if destination_width <= source_width && destination_height <= source_height {
                sample_area(
                    image,
                    x,
                    y,
                    source_width,
                    source_height,
                    destination_width,
                    destination_height,
                )
            } else {
                sample_bilinear(
                    image,
                    x,
                    y,
                    source_width,
                    source_height,
                    destination_width,
                    destination_height,
                )
            }
        }
    }
}

fn sample_nearest(
    image: &dyn ImageResource,
    x: u32,
    y: u32,
    source_width: u32,
    source_height: u32,
    destination_width: u32,
    destination_height: u32,
) -> Option<Color> {
    let source_x =
        u64::from(x).saturating_mul(u64::from(source_width)) / u64::from(destination_width);
    let source_y =
        u64::from(y).saturating_mul(u64::from(source_height)) / u64::from(destination_height);

    let source_x = source_x.min(u64::from(source_width.saturating_sub(1)));
    let source_y = source_y.min(u64::from(source_height.saturating_sub(1)));

    image.pixel(u32::try_from(source_x).ok()?, u32::try_from(source_y).ok()?)
}

fn sample_bilinear(
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

#[allow(clippy::too_many_arguments)]
fn sample_area(
    image: &dyn ImageResource,
    x: u32,
    y: u32,
    source_width: u32,
    source_height: u32,
    destination_width: u32,
    destination_height: u32,
) -> Option<Color> {
    // tse one exact integer coordinate system per axis.
    // On X:
    //   source pixel i:
    //     [i * destination_width, (i + 1) * destination_width)
    //   destination pixel x:
    //     [x * source_width, (x + 1) * source_width)
    // their intersection length is therefore an exact integer.
    // Y works identically. Multiplying both overlap lengths gives the exact area
    // weight of a source pixel.
    let destination_width_u64 = u64::from(destination_width);
    let destination_height_u64 = u64::from(destination_height);

    let x_start = u64::from(x) * u64::from(source_width);
    let x_end = (u64::from(x) + 1) * u64::from(source_width);

    let y_start = u64::from(y) * u64::from(source_height);
    let y_end = (u64::from(y) + 1) * u64::from(source_height);

    let first_source_x = u32::try_from(x_start / destination_width_u64)
        .ok()?
        .min(source_width.saturating_sub(1));
    let first_source_y = u32::try_from(y_start / destination_height_u64)
        .ok()?
        .min(source_height.saturating_sub(1));

    let end_source_x =
        u32::try_from(ceil_div(x_end, destination_width_u64).min(u64::from(source_width))).ok()?;
    let end_source_y =
        u32::try_from(ceil_div(y_end, destination_height_u64).min(u64::from(source_height)))
            .ok()?;

    let mut red = 0u128;
    let mut green = 0u128;
    let mut blue = 0u128;
    let mut total_weight = 0u128;

    for source_y in first_source_y..end_source_y {
        let source_top = u64::from(source_y) * destination_height_u64;
        let source_bottom = (u64::from(source_y) + 1) * destination_height_u64;

        let y_weight = overlap_length(y_start, y_end, source_top, source_bottom);

        if y_weight == 0 {
            continue;
        }

        for source_x in first_source_x..end_source_x {
            let source_left = u64::from(source_x) * destination_width_u64;
            let source_right = (u64::from(source_x) + 1) * destination_width_u64;

            let x_weight = overlap_length(x_start, x_end, source_left, source_right);
            if x_weight == 0 {
                continue;
            }

            let color = image.pixel(source_x, source_y)?;
            let weight = u128::from(x_weight) * u128::from(y_weight);

            total_weight += weight;
            red += u128::from(color.r) * weight;
            green += u128::from(color.g) * weight;
            blue += u128::from(color.b) * weight;
        }
    }

    if total_weight == 0 {
        return None;
    }

    Some(Color::rgb(
        weighted_channel(red, total_weight),
        weighted_channel(green, total_weight),
        weighted_channel(blue, total_weight),
    ))
}

fn overlap_length(first_start: u64, first_end: u64, second_start: u64, second_end: u64) -> u64 {
    min(first_end, second_end).saturating_sub(max(first_start, second_start))
}

fn ceil_div(value: u64, divisor: u64) -> u64 {
    debug_assert!(divisor > 0);
    value / divisor + u64::from(!value.is_multiple_of(divisor))
}

fn weighted_channel(weighted_sum: u128, total_weight: u128) -> u8 {
    let rounded = weighted_sum.saturating_add(total_weight / 2) / total_weight;
    u8::try_from(rounded).unwrap_or(u8::MAX)
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
        lerp_channel(first.r, second.r, fraction),
        lerp_channel(first.g, second.g, fraction),
        lerp_channel(first.b, second.b, fraction),
    )
}

fn lerp_channel(first: u8, second: u8, fraction: u16) -> u8 {
    let fraction = u32::from(fraction);
    let inverse = 256 - fraction;
    let value = (u32::from(first) * inverse + u32::from(second) * fraction + 128) / 256;

    u8::try_from(value).unwrap_or(u8::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::{Size, px};

    struct WhiteCornerImage;

    impl ImageResource for WhiteCornerImage {
        fn size(&self) -> Size {
            Size::new(px(4), px(4))
        }

        fn pixel(&self, x: u32, y: u32) -> Option<Color> {
            if x >= 4 || y >= 4 {
                return None;
            }

            if x == 0 && y == 0 {
                Some(Color::WHITE)
            } else {
                Some(Color::BLACK)
            }
        }
    }

    struct ThreePixelStrip;

    impl ImageResource for ThreePixelStrip {
        fn size(&self) -> Size {
            Size::new(px(3), px(1))
        }

        fn pixel(&self, x: u32, y: u32) -> Option<Color> {
            if y != 0 {
                return None;
            }

            match x {
                0 | 2 => Some(Color::BLACK),
                1 => Some(Color::WHITE),
                _ => None,
            }
        }
    }

    struct FourColorImage;

    impl ImageResource for FourColorImage {
        fn size(&self) -> Size {
            Size::new(px(2), px(2))
        }

        fn pixel(&self, x: u32, y: u32) -> Option<Color> {
            match (x, y) {
                (0, 0) => Some(Color::BLACK),
                (1, 0) => Some(Color::RED),
                (0, 1) => Some(Color::GREEN),
                (1, 1) => Some(Color::BLUE),
                _ => None,
            }
        }
    }

    #[test]
    fn area_sampling_averages_entire_source_footprint() {
        let image = WhiteCornerImage;

        let color = sample_image(&image, 0, 0, 4, 4, 1, 1, ImageSampling::Area).unwrap();

        // one of sixteen source pixels is white.
        // 255 / 16 = 15.9375,  rounded to 16.
        assert_eq!(color, Color::rgb(16, 16, 16));
    }

    #[test]
    fn area_sampling_weights_partial_source_pixels() {
        let image = ThreePixelStrip;

        let left = sample_image(&image, 0, 0, 3, 1, 2, 1, ImageSampling::Area).unwrap();
        let right = sample_image(&image, 1, 0, 3, 1, 2, 1, ImageSampling::Area).unwrap();

        // destination 0 covers:
        //   source 0: weight 2
        //   source 1: weight 1
        // destination 1 covers:
        //   source 1: weight 1
        //   source 2: weight 2
        // therefore both produce:
        // 255 / 3 = 85.
        let expected = Color::rgb(85, 85, 85);

        assert_eq!(left, expected);
        assert_eq!(right, expected);
    }

    #[test]
    fn area_sampling_preserves_native_pixels() {
        let image = ThreePixelStrip;

        assert_eq!(
            sample_image(&image, 0, 0, 3, 1, 3, 1, ImageSampling::Area),
            Some(Color::BLACK),
        );
        assert_eq!(
            sample_image(&image, 1, 0, 3, 1, 3, 1, ImageSampling::Area),
            Some(Color::WHITE),
        );
        assert_eq!(
            sample_image(&image, 2, 0, 3, 1, 3, 1, ImageSampling::Area),
            Some(Color::BLACK),
        );
    }

    #[test]
    fn area_sampling_falls_back_to_bilinear_when_magnifying() {
        let image = FourColorImage;

        for y in 0..3 {
            for x in 0..3 {
                let area = sample_image(&image, x, y, 2, 2, 3, 3, ImageSampling::Area);

                let bilinear = sample_image(&image, x, y, 2, 2, 3, 3, ImageSampling::Bilinear);

                assert_eq!(area, bilinear, "area fallback differs at ({x}, {y})");
            }
        }
    }
}
