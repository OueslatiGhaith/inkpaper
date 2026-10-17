use embedded_graphics::{
    Pixel as EgPixel, geometry::Point as EgPoint, pixelcolor::GrayColor,
    prelude::DrawTarget as EgDrawTarget,
};

use crate::{
    Color, ImagePaint, ImageResource, Point, Rect,
    backend::eink::EInkTone,
    rendering::image_paint::{PixelSink, paint_image},
};

use super::{Gray2, coverage::color_to_gray2};

pub(super) fn draw_image_to<D>(
    target: &mut D,
    image: &dyn ImageResource,
    bounds: Rect,
    paint: ImagePaint,
    clip: Option<Rect>,
) -> Result<EInkTone, D::Error>
where
    D: EgDrawTarget<Color = Gray2>,
{
    let mut sink = EInkPixelSink {
        target,
        tone: EInkTone::Binary,
    };

    paint_image(&mut sink, image, bounds, paint, clip)?;

    Ok(sink.tone)
}

struct EInkPixelSink<'a, D> {
    target: &'a mut D,
    tone: EInkTone,
}

impl<D> PixelSink for EInkPixelSink<'_, D>
where
    D: EgDrawTarget<Color = Gray2>,
{
    type Error = D::Error;

    fn draw_pixels(&mut self, origin: Point, pixels: &[Color]) -> Result<(), Self::Error> {
        let Self { target, tone } = self;
        let y = origin.y.get();

        let pixels = pixels.iter().zip(origin.x.get()..).map(|(&color, x)| {
            let gray = color_to_gray2(color);

            if matches!(gray.luma(), 1 | 2) {
                *tone = EInkTone::Gray4;
            }

            EgPixel(EgPoint::new(x, y), gray)
        });

        target.draw_iter(pixels)
    }
}

