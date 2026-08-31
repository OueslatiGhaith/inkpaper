use core::marker::PhantomData;

use embedded_graphics::{
    Drawable, Pixel as EgPixel,
    draw_target::DrawTargetExt,
    geometry::Point as EgPoint,
    image::{
        GetPixel as EgGetPixel, Image as EgImage,
        ImageDrawable as EgImageDrawable,
    },
    prelude::DrawTarget as EgDrawTarget,
};

use crate::{
    ImageFit, ImageId, ImageSource, Point, Rect, Size, fitted_image_bounds,
};

use super::{from_embedded_size, to_embedded_point, to_embedded_rect};

pub struct EmbeddedGraphicsImage<'image, D>
where
    D: EgDrawTarget,
{
    image: *const (),
    size: Size,
    #[allow(clippy::type_complexity)]
    draw_fn: unsafe fn(
        image: *const (),
        target: &mut D,
        origin: Point,
        clip: Option<Rect>,
    ) -> Result<(), D::Error>,
    #[allow(clippy::type_complexity)]
    draw_scaled_fn: unsafe fn(
        image: *const (),
        target: &mut D,
        destination: Rect,
        clip: Option<Rect>,
    ) -> Result<(), D::Error>,
    _lifetime: PhantomData<&'image ()>,
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

impl<'image, D> EmbeddedGraphicsImage<'image, D>
where
    D: EgDrawTarget,
{
    pub fn new<T>(image: &'image T) -> Self
    where
        T: EgImageDrawable + EgGetPixel<Color = <T as EgImageDrawable>::Color>,
        <T as EgImageDrawable>::Color: Into<D::Color>,
    {
        Self {
            image: core::ptr::from_ref(image).cast(),
            size: from_embedded_size(image.size()),
            draw_fn: draw_erased_image::<T, D>,
            draw_scaled_fn: draw_erased_scaled_image::<T, D>,
            _lifetime: PhantomData,
        }
    }

    pub const fn size(self) -> Size {
        self.size
    }

    pub const fn source(self, id: ImageId) -> ImageSource {
        ImageSource::new(id, self.size)
    }

    pub(super) fn draw(
        self,
        target: &mut D,
        bounds: Rect,
        fit: ImageFit,
        clip: Option<Rect>,
    ) -> Result<(), D::Error> {
        let destination = fitted_image_bounds(self.size, bounds, fit);
        if destination.width().is_non_positive() || destination.height().is_non_positive() {
            return Ok(());
        }
        if fit == ImageFit::None || destination.size == self.size {
            return unsafe { (self.draw_fn)(self.image, target, destination.origin, clip) };
        }

        unsafe { (self.draw_scaled_fn)(self.image, target, destination, clip) }
    }
}

unsafe fn draw_erased_image<T, D>(
    image: *const (),
    target: &mut D,
    origin: Point,
    clip: Option<Rect>,
) -> Result<(), D::Error>
where
    D: EgDrawTarget,
    T: EgImageDrawable,
    T::Color: Into<D::Color>,
{
    let image = unsafe { &*image.cast::<T>() };
    let position = to_embedded_point(origin);
    let drawable = EgImage::new(image, position);
    let mut target = target.color_converted::<T::Color>();

    match clip {
        Some(clip) => {
            let clip = to_embedded_rect(clip);
            let mut clipped = target.clipped(&clip);
            drawable.draw(&mut clipped).map(|_| ())
        }
        None => drawable.draw(&mut target).map(|_| ()),
    }
}

unsafe fn draw_erased_scaled_image<T, D>(
    image: *const (),
    target: &mut D,
    destination: Rect,
    clip: Option<Rect>,
) -> Result<(), D::Error>
where
    D: EgDrawTarget,
    T: EgImageDrawable + EgGetPixel<Color = <T as EgImageDrawable>::Color>,
    <T as EgImageDrawable>::Color: Into<D::Color>,
{
    let image = unsafe { &*image.cast::<T>() };
    let source_size = image.size();
    if source_size.width == 0
        || source_size.height == 0
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

    let destination_x = destination.x().get();
    let destination_y = destination.y().get();
    let destination_width = destination.width().get().max(1);
    let destination_height = destination.height().get().max(1);

    let left = visible.x().get();
    let top = visible.y().get();
    let right = visible.right().get();
    let bottom = visible.bottom().get();

    let source_width = u64::from(source_size.width);
    let source_height = u64::from(source_size.height);

    let pixels = (top..bottom).flat_map(|y| {
        (left..right).filter_map(move |x| {
            let relative_x = i64::from(x) - i64::from(destination_x);
            let relative_y = i64::from(y) - i64::from(destination_y);
            if relative_x < 0 || relative_y < 0 {
                return None;
            }

            let source_x = (u64::try_from(relative_x)
                .unwrap_or(0)
                .saturating_mul(source_width)
                / u64::try_from(destination_width).unwrap_or(1))
            .min(source_width.saturating_sub(1));

            let source_y = (u64::try_from(relative_y)
                .unwrap_or(0)
                .saturating_mul(source_height)
                / u64::try_from(destination_height).unwrap_or(1))
            .min(source_height.saturating_sub(1));

            let source_point = EgPoint::new(
                i32::try_from(source_x).unwrap_or(i32::MAX),
                i32::try_from(source_y).unwrap_or(i32::MAX),
            );

            image
                .pixel(source_point)
                .map(|color| EgPixel(EgPoint::new(x, y), color))
        })
    });

    let mut target = target.color_converted::<<T as EgImageDrawable>::Color>();

    target.draw_iter(pixels)
}
