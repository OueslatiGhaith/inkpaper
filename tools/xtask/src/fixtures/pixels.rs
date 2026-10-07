use anyhow::Result;

pub fn checkerboard_png(width: u32, height: u32, cell_size: u32) -> Result<Vec<u8>> {
    let mut pixels = Vec::with_capacity(width as usize * height as usize * 3);

    for y in 0..height {
        for x in 0..width {
            let light = ((x / cell_size) + (y / cell_size)).is_multiple_of(2);
            let value = if light { 224 } else { 32 };

            pixels.extend_from_slice(&[value, value, value]);
        }
    }

    encode_png(width, height, png::ColorType::Rgb, &pixels)
}

pub fn gradient_png(width: u32, height: u32) -> Result<Vec<u8>> {
    let mut pixels = Vec::with_capacity(width as usize * height as usize * 3);

    for y in 0..height {
        for x in 0..width {
            let horizontal = if width <= 1 {
                0
            } else {
                ((x * 255) / (width - 1)) as u8
            };

            let vertical = if height <= 1 {
                0
            } else {
                ((y * 255) / (height - 1)) as u8
            };

            let mixed = ((u16::from(horizontal) + u16::from(vertical)) / 2) as u8;

            pixels.extend_from_slice(&[horizontal, vertical, mixed]);
        }
    }

    encode_png(width, height, png::ColorType::Rgb, &pixels)
}

pub fn encode_png(
    width: u32,
    height: u32,
    color_type: png::ColorType,
    pixels: &[u8],
) -> Result<Vec<u8>> {
    let mut output = Vec::new();

    {
        let mut encoder = png::Encoder::new(&mut output, width, height);

        encoder.set_color(color_type);
        encoder.set_depth(png::BitDepth::Eight);

        let mut writer = encoder.write_header()?;

        writer.write_image_data(pixels)?;
        writer.finish()?;
    }

    Ok(output)
}

/// A book cover: a black frame, the title in white on a black band and the
/// subtitle below it, with a gradient at the foot. The text is drawn in a
/// 10 × 20 font scaled up, so it stays readable as a home screen thumbnail
pub fn cover_png(width: u32, height: u32, title: &str, subtitle: &str) -> Result<Vec<u8>> {
    let mut cover = GrayCanvas::new(width, height, 255);

    let frame = width / 30;
    let band_top = height * 3 / 20;
    let band_bottom = height * 9 / 20;

    cover.fill(0, 0, width, height, 0);
    cover.fill(frame, frame, width - 2 * frame, height - 2 * frame, 255);
    cover.fill(
        frame,
        band_top,
        width - 2 * frame,
        band_bottom - band_top,
        0,
    );

    let scale = (width - 4 * frame) / (10 * title.chars().count().max(1) as u32);
    let title_height = 20 * scale;
    cover.text(
        title,
        width / 2,
        (band_top + band_bottom - title_height) / 2,
        scale,
        255,
    );

    let subtitle_scale = (scale / 2).max(1);
    cover.text(
        subtitle,
        width / 2,
        band_bottom + height / 12,
        subtitle_scale,
        0,
    );

    let foot = height * 3 / 4;
    for y in foot..height - frame {
        for x in frame..width - frame {
            let shade = (x - frame) * 255 / (width - 2 * frame);
            cover.set(x, y, shade as u8);
        }
    }

    encode_png(width, height, png::ColorType::Grayscale, &cover.pixels)
}

struct GrayCanvas {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl GrayCanvas {
    fn new(width: u32, height: u32, value: u8) -> Self {
        Self {
            width,
            height,
            pixels: vec![value; width as usize * height as usize],
        }
    }

    fn set(&mut self, x: u32, y: u32, value: u8) {
        if x < self.width && y < self.height {
            self.pixels[(y * self.width + x) as usize] = value;
        }
    }

    fn fill(&mut self, left: u32, top: u32, width: u32, height: u32, value: u8) {
        for y in top..top + height {
            for x in left..left + width {
                self.set(x, y, value);
            }
        }
    }

    /// `text` centred on `center`, its top at `top`, each font pixel a
    /// `scale` × `scale` square
    fn text(&mut self, text: &str, center: u32, top: u32, scale: u32, value: u8) {
        use embedded_graphics::{
            mock_display::MockDisplay,
            mono_font::{MonoTextStyle, ascii::FONT_10X20},
            pixelcolor::BinaryColor,
            prelude::*,
            text::{Baseline, Text},
        };

        let width = 10 * text.chars().count() as u32;
        let left = center.saturating_sub(width * scale / 2);

        // the mock display is 64 × 64, so draw a few characters at a time
        for (chunk_index, chunk) in text.as_bytes().chunks(6).enumerate() {
            let chunk = core::str::from_utf8(chunk).unwrap_or("");
            let mut display = MockDisplay::<BinaryColor>::new();
            display.set_allow_out_of_bounds_drawing(true);

            Text::with_baseline(
                chunk,
                Point::zero(),
                MonoTextStyle::new(&FONT_10X20, BinaryColor::On),
                Baseline::Top,
            )
            .draw(&mut display)
            .ok();

            let offset = chunk_index as u32 * 60;

            for y in 0..20 {
                for x in 0..60 {
                    if display.get_pixel(Point::new(x, y)) != Some(BinaryColor::On) {
                        continue;
                    }

                    let x = left + (offset + x as u32) * scale;
                    let y = top + y as u32 * scale;
                    self.fill(x, y, scale, scale, value);
                }
            }
        }
    }
}
