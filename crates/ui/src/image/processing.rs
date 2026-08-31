use crate::{Color, ImageColorMode, ImageDither, ImagePaint};

#[rustfmt::skip]
const BAYER_2X2: [u8; 4] = [
    0, 2,
    3, 1,
];

#[rustfmt::skip]
const BAYER_4X4: [u8; 16] = [
    0, 8, 2, 10,
    12, 4, 14, 6,
    3, 11, 1, 9,
    15, 7, 13, 5,
];

pub(crate) fn process_image_pixel(color: Color, paint: ImagePaint, x: i32, y: i32) -> Color {
    match paint.color_mode {
        ImageColorMode::Color => process_color(color, paint),
        ImageColorMode::Grayscale => {
            let luminance = adjusted_luminance(color, paint);
            Color::rgb(luminance, luminance, luminance)
        }
        ImageColorMode::Monochrome => {
            let luminance = adjusted_luminance(color, paint);

            if dithered_black(luminance, paint.dither, x, y) {
                Color::BLACK
            } else {
                Color::WHITE
            }
        }
    }
}

fn process_color(color: Color, paint: ImagePaint) -> Color {
    let mut red = adjust_channel(color.r, paint.brightness, paint.contrast);
    let mut green = adjust_channel(color.g, paint.brightness, paint.contrast);
    let mut blue = adjust_channel(color.b, paint.brightness, paint.contrast);

    if paint.invert {
        red = 255 - red;
        green = 255 - green;
        blue = 255 - blue;
    }

    Color::rgb(red, green, blue)
}

fn adjusted_luminance(color: Color, paint: ImagePaint) -> u8 {
    let luminance = luminance(color);
    let adjusted = adjust_channel(luminance, paint.brightness, paint.contrast);

    if paint.invert {
        255 - adjusted
    } else {
        adjusted
    }
}

fn luminance(color: Color) -> u8 {
    // Rec. 601 integer luminance.
    // the coefficients sum to 256, which lets us stay entirely in fixed-point integer arithmetic.
    let value =
        77u32 * u32::from(color.r) + 150u32 * u32::from(color.g) + 29u32 * u32::from(color.b) + 128;

    u8::try_from(value / 256).unwrap_or(u8::MAX)
}

fn adjust_channel(value: u8, brightness: i16, contrast: u16) -> u8 {
    let centered = i32::from(value) - 128;
    let contrasted = centered.saturating_mul(i32::from(contrast)) / 100;
    let adjusted = contrasted + 128 + i32::from(brightness);

    u8::try_from(adjusted.clamp(0, 255)).unwrap_or(if adjusted < 0 { 0 } else { 255 })
}

fn dithered_black(luminance: u8, dither: ImageDither, x: i32, y: i32) -> bool {
    match dither {
        ImageDither::Threshold => luminance < 128,
        ImageDither::Bayer2x2 => ordered_dither_black(luminance, x, y, &BAYER_2X2, 2),
        ImageDither::Bayer4x4 => ordered_dither_black(luminance, x, y, &BAYER_4X4, 4),
    }
}

fn ordered_dither_black(luminance: u8, x: i32, y: i32, matrix: &[u8], matrix_size: i32) -> bool {
    let matrix_x = usize::try_from(x.rem_euclid(matrix_size)).unwrap_or(0);
    let matrix_y = usize::try_from(y.rem_euclid(matrix_size)).unwrap_or(0);
    let matrix_size_usize = usize::try_from(matrix_size).unwrap_or(1);
    let rank = u32::from(matrix[matrix_y * matrix_size_usize + matrix_x]);
    let levels = u32::try_from(matrix.len()).unwrap_or(1);

    // place each threshold at the center of its quantization bucket.
    // 4x4 therefore produces:
    // 8, 24, 40, ... 248
    let threshold = ((rank * 2 + 1) * 256) / (levels * 2);

    u32::from(luminance) < threshold
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grayscale_uses_rec_601_luminance() {
        let paint = ImagePaint {
            color_mode: ImageColorMode::Grayscale,
            ..ImagePaint::default()
        };

        assert_eq!(
            process_image_pixel(Color::RED, paint, 0, 0),
            Color::rgb(77, 77, 77),
        );
    }

    #[test]
    fn color_mode_preserves_color_with_neutral_adjustments() {
        let color = Color::rgb(37, 128, 221);

        assert_eq!(
            process_image_pixel(color, ImagePaint::default(), 0, 0,),
            color,
        );
    }

    #[test]
    fn brightness_and_contrast_adjust_luminance() {
        let paint = ImagePaint {
            color_mode: ImageColorMode::Grayscale,
            brightness: 10,
            contrast: 200,
            ..ImagePaint::default()
        };

        assert_eq!(
            process_image_pixel(Color::rgb(100, 100, 100), paint, 0, 0,),
            Color::rgb(82, 82, 82),
        );
    }

    #[test]
    fn inversion_is_applied_after_tone_adjustment() {
        let paint = ImagePaint {
            color_mode: ImageColorMode::Grayscale,
            invert: true,
            ..ImagePaint::default()
        };

        assert_eq!(
            process_image_pixel(Color::rgb(40, 40, 40), paint, 0, 0,),
            Color::rgb(215, 215, 215),
        );
    }

    #[test]
    fn threshold_monochrome_produces_only_binary_colors() {
        let paint = ImagePaint {
            color_mode: ImageColorMode::Monochrome,
            dither: ImageDither::Threshold,
            ..ImagePaint::default()
        };

        assert_eq!(
            process_image_pixel(Color::rgb(127, 127, 127), paint, 0, 0,),
            Color::BLACK,
        );

        assert_eq!(
            process_image_pixel(Color::rgb(128, 128, 128), paint, 0, 0,),
            Color::WHITE,
        );
    }

    #[test]
    fn bayer_2x2_half_gray_is_half_black() {
        let paint = ImagePaint {
            color_mode: ImageColorMode::Monochrome,
            dither: ImageDither::Bayer2x2,
            ..ImagePaint::default()
        };

        let mut black = 0;

        for y in 0..2 {
            for x in 0..2 {
                if process_image_pixel(Color::rgb(128, 128, 128), paint, x, y) == Color::BLACK {
                    black += 1;
                }
            }
        }

        assert_eq!(black, 2);
    }

    #[test]
    fn bayer_4x4_half_gray_is_half_black() {
        let paint = ImagePaint {
            color_mode: ImageColorMode::Monochrome,
            dither: ImageDither::Bayer4x4,
            ..ImagePaint::default()
        };

        let mut black = 0;

        for y in 0..4 {
            for x in 0..4 {
                if process_image_pixel(Color::rgb(128, 128, 128), paint, x, y) == Color::BLACK {
                    black += 1;
                }
            }
        }

        assert_eq!(black, 8);
    }

    #[test]
    fn bayer_4x4_repeats_in_absolute_coordinates() {
        let paint = ImagePaint {
            color_mode: ImageColorMode::Monochrome,
            dither: ImageDither::Bayer4x4,
            ..ImagePaint::default()
        };

        for y in 0..4 {
            for x in 0..4 {
                assert_eq!(
                    process_image_pixel(Color::rgb(93, 93, 93), paint, x, y,),
                    process_image_pixel(Color::rgb(93, 93, 93), paint, x + 4, y + 4,),
                );
            }
        }
    }
}
