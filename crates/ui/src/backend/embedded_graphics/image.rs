use embedded_graphics::{
    geometry::Point as EgPoint,
    image::{GetPixel as EgGetPixel, ImageDrawable as EgImageDrawable},
    pixelcolor::{Rgb888 as EgRgb888, RgbColor},
};

use crate::{Color, ImageResource, Size};

use super::from_embedded_size;

pub struct EmbeddedGraphicsImage<'image, T> {
    image: &'image T,
}

impl<T> Copy for EmbeddedGraphicsImage<'_, T> {}
impl<T> Clone for EmbeddedGraphicsImage<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'image, T> EmbeddedGraphicsImage<'image, T> {
    pub const fn new(image: &'image T) -> Self {
        Self { image }
    }

    pub const fn image(self) -> &'image T {
        self.image
    }
}

impl<T> ImageResource for EmbeddedGraphicsImage<'_, T>
where
    T: EgImageDrawable + EgGetPixel<Color = <T as EgImageDrawable>::Color>,
    <T as EgImageDrawable>::Color: Into<EgRgb888>,
{
    fn size(&self) -> Size {
        from_embedded_size(self.image.size())
    }

    fn pixel(&self, x: u32, y: u32) -> Option<Color> {
        let x = i32::try_from(x).ok()?;
        let y = i32::try_from(y).ok()?;

        let color: EgRgb888 = self.image.pixel(EgPoint::new(x, y))?.into();

        Some(Color::rgb(color.r(), color.g(), color.b()))
    }
}
