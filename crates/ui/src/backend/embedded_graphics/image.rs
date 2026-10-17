use embedded_graphics::{
    Pixel as EgPixel,
    draw_target::DrawTargetExt,
    geometry::Point as EgPoint,
    image::{GetPixel as EgGetPixel, ImageDrawable as EgImageDrawable},
    pixelcolor::{Rgb888 as EgRgb888, RgbColor},
    prelude::DrawTarget as EgDrawTarget,
};

use crate::{
    Color, ImagePaint, ImageResource, Point, Rect, Size,
    backend::embedded_graphics::to_rgb888,
    rendering::image_paint::{PixelSink, paint_image},
};

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

pub(super) fn draw_image_to<D>(
    target: &mut D,
    image: &dyn ImageResource,
    bounds: Rect,
    paint: ImagePaint,
    clip: Option<Rect>,
) -> Result<(), D::Error>
where
    D: EgDrawTarget,
    D::Color: From<EgRgb888>,
{
    paint_image(
        &mut EmbeddedGraphicsPixelSink { target },
        image,
        bounds,
        paint,
        clip,
    )
}

struct EmbeddedGraphicsPixelSink<'a, D> {
    target: &'a mut D,
}

impl<D> PixelSink for EmbeddedGraphicsPixelSink<'_, D>
where
    D: EgDrawTarget,
    D::Color: From<EgRgb888>,
{
    type Error = D::Error;

    fn draw_pixels(&mut self, origin: Point, pixels: &[Color]) -> Result<(), Self::Error> {
        let y = origin.y.get();

        let pixels = pixels
            .iter()
            .zip(origin.x.get()..)
            .map(|(&color, x)| EgPixel(EgPoint::new(x, y), to_rgb888(color)));

        self.target.color_converted::<EgRgb888>().draw_iter(pixels)
    }
}
