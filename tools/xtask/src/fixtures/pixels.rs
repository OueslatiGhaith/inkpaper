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
