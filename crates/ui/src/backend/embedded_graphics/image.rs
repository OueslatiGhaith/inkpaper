use embedded_graphics::{
    Pixel as EgPixel,
    draw_target::DrawTargetExt,
    geometry::Point as EgPoint,
    image::{GetPixel as EgGetPixel, ImageDrawable as EgImageDrawable},
    pixelcolor::{Rgb888 as EgRgb888, RgbColor},
    prelude::DrawTarget as EgDrawTarget,
};

use crate::{
    Color, ImageFit, ImageResource, Rect, Size, backend::embedded_graphics::to_rgb888,
    fitted_image_bounds,
};

use super::from_embedded_size;

pub struct EmbeddedGraphicsImage<'image, T> {
    image: &'image T,
}

impl<'image, D> Copy for EmbeddedGraphicsImage<'image, D> where D: EgDrawTarget {}
impl<'image, D> Clone for EmbeddedGraphicsImage<'image, D>
where
    D: EgDrawTarget,
{
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
    fit: ImageFit,
    clip: Option<Rect>,
) -> Result<(), D::Error>
where
    D: EgDrawTarget,
    D::Color: From<EgRgb888>,
{
    let source_size = image.size();
    let destination = fitted_image_bounds(source_size, bounds, fit);

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

    let source_width = u64::try_from(source_size.width.get()).unwrap_or(0);
    let source_height = u64::try_from(source_size.height.get()).unwrap_or(0);
    if source_width == 0 || source_height == 0 {
        return Ok(());
    }

    let destination_x = destination.x().get();
    let destination_y = destination.y().get();
    let destination_width = destination.width().get().max(1);
    let destination_height = destination.height().get().max(1);

    let left = visible.x().get();
    let top = visible.y().get();
    let right = visible.right().get();
    let bottom = visible.bottom().get();

    let pixels = (top..bottom).flat_map(|y| {
        (left..right).filter_map(move |x| {
            let relative_x = i64::from(x) - i64::from(destination_x);
            let relative_y = i64::from(y) - i64::from(destination_y);
            if relative_x < 0 || relative_y < 0 {
                return None;
            }

            let source_x = u64::try_from(relative_x)
                .unwrap_or(0)
                .saturating_mul(source_width)
                / u64::try_from(destination_width).unwrap_or(1);

            let source_y = u64::try_from(relative_y)
                .unwrap_or(0)
                .saturating_mul(source_height)
                / u64::try_from(destination_height).unwrap_or(1);

            let source_x = source_x.min(source_width.saturating_sub(1));
            let source_y = source_y.min(source_height.saturating_sub(1));

            let source_x = u32::try_from(source_x).ok()?;
            let source_y = u32::try_from(source_y).ok()?;

            image
                .pixel(source_x, source_y)
                .map(|color| EgPixel(EgPoint::new(x, y), to_rgb888(color)))
        })
    });

    let mut target = target.color_converted::<EgRgb888>();

    target.draw_iter(pixels)
}
