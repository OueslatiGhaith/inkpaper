use std::path::Path;

use image::RgbImage;
use inkpaper_ui::{Color, ImageResource, Size, px};

pub struct HostImage {
    pixels: RgbImage,
}

impl HostImage {
    pub fn open(path: &Path) -> Result<Self, image::ImageError> {
        let pixels = image::open(path)?.to_rgb8();

        Ok(Self { pixels })
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, image::ImageError> {
        let pixels = image::load_from_memory(bytes)?.to_rgb8();

        Ok(Self { pixels })
    }
}

impl ImageResource for HostImage {
    fn size(&self) -> Size {
        Size::new(
            px(i32::try_from(self.pixels.width()).unwrap_or(i32::MAX)),
            px(i32::try_from(self.pixels.height()).unwrap_or(i32::MAX)),
        )
    }

    fn pixel(&self, x: u32, y: u32) -> Option<Color> {
        if x >= self.pixels.width() || y >= self.pixels.height() {
            return None;
        }

        let pixel = self.pixels.get_pixel(x, y);

        Some(Color::rgb(pixel[0], pixel[1], pixel[2]))
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use image::{DynamicImage, ImageFormat, Rgb};

    use super::*;

    #[test]
    fn host_image_exposes_dimensions_and_pixels() {
        let mut pixels = RgbImage::new(2, 1);
        pixels.put_pixel(0, 0, Rgb([10, 20, 30]));
        pixels.put_pixel(1, 0, Rgb([200, 210, 220]));

        let image = HostImage { pixels };

        assert_eq!(image.size(), Size::new(px(2), px(1)));
        assert_eq!(image.pixel(0, 0), Some(Color::rgb(10, 20, 30)));
        assert_eq!(image.pixel(1, 0), Some(Color::rgb(200, 210, 220)));
        assert_eq!(image.pixel(2, 0), None);
        assert_eq!(image.pixel(0, 1), None);
    }

    #[test]
    fn host_image_decodes_png_bytes() {
        let mut pixels = RgbImage::new(2, 1);
        pixels.put_pixel(0, 0, Rgb([12, 34, 56]));
        pixels.put_pixel(1, 0, Rgb([78, 90, 123]));

        let mut encoded = Cursor::new(Vec::new());

        DynamicImage::ImageRgb8(pixels)
            .write_to(&mut encoded, ImageFormat::Png)
            .unwrap();

        let image = HostImage::decode(encoded.get_ref()).unwrap();

        assert_eq!(image.size(), Size::new(px(2), px(1)));
        assert_eq!(image.pixel(0, 0), Some(Color::rgb(12, 34, 56)));
        assert_eq!(image.pixel(1, 0), Some(Color::rgb(78, 90, 123)));
    }
}
