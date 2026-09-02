use embedded_graphics::{
    Pixel as EgPixel,
    draw_target::DrawTargetExt,
    geometry::Point as EgPoint,
    image::{GetPixel as EgGetPixel, ImageDrawable as EgImageDrawable},
    pixelcolor::{Rgb888 as EgRgb888, RgbColor},
    prelude::DrawTarget as EgDrawTarget,
};

use crate::{
    Color, ImagePaint, ImageResource, Rect, Size, backend::embedded_graphics::to_rgb888,
    fitted_image_bounds, process_image_pixel, sample_image,
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
    let source_size = image.size();
    let destination = fitted_image_bounds(source_size, bounds, paint.fit, paint.position);

    if source_size.width.is_non_positive()
        || source_size.height.is_non_positive()
        || destination.width().is_non_positive()
        || destination.height().is_non_positive()
    {
        return Ok(());
    }

    let Some(visible) = (match clip {
        Some(clip) => destination.intersection(clip),
        None => Some(destination),
    }) else {
        return Ok(());
    };

    let source_width = u32::try_from(source_size.width.get()).unwrap_or(0);
    let source_height = u32::try_from(source_size.height.get()).unwrap_or(0);
    let destination_width = u32::try_from(destination.width().get()).unwrap_or(0);
    let destination_height = u32::try_from(destination.height().get()).unwrap_or(0);

    if source_width == 0 || source_height == 0 || destination_width == 0 || destination_height == 0
    {
        return Ok(());
    }

    let destination_x = destination.x().get();
    let destination_y = destination.y().get();

    let left = visible.x().get();
    let top = visible.y().get();
    let right = visible.right().get();
    let bottom = visible.bottom().get();

    let pixels = (top..bottom).flat_map(|y| {
        (left..right).filter_map(move |x| {
            let relative_x = i64::from(x) - i64::from(destination_x);
            let relative_y = i64::from(y) - i64::from(destination_y);

            let relative_x = u32::try_from(relative_x).ok()?;
            let relative_y = u32::try_from(relative_y).ok()?;

            let color = sample_image(
                image,
                relative_x,
                relative_y,
                source_width,
                source_height,
                destination_width,
                destination_height,
                paint.sampling,
            )?;

            let color = process_image_pixel(color, paint, x, y);

            Some(EgPixel(EgPoint::new(x, y), to_rgb888(color)))
        })
    });

    let mut target = target.color_converted::<EgRgb888>();

    target.draw_iter(pixels)
}
