use core::cell::Cell;

use embedded_graphics::{
    Pixel as EgPixel, geometry::Point as EgPoint, pixelcolor::GrayColor,
    prelude::DrawTarget as EgDrawTarget,
};

use crate::{
    ImagePaint, ImageResource, ImageSampler, Rect, backend::eink::EInkTone, fitted_image_bounds,
    process_image_pixel,
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
    let source_size = image.size();

    let destination = fitted_image_bounds(source_size, bounds, paint.fit, paint.position);

    if source_size.width.is_non_positive()
        || source_size.height.is_non_positive()
        || destination.width().is_non_positive()
        || destination.height().is_non_positive()
    {
        return Ok(EInkTone::Binary);
    }

    let Some(visible) = (match clip {
        Some(clip) => destination.intersection(clip),
        None => Some(destination),
    }) else {
        return Ok(EInkTone::Binary);
    };

    let source_width = u32::try_from(source_size.width.get()).unwrap_or(0);
    let source_height = u32::try_from(source_size.height.get()).unwrap_or(0);

    let destination_width = u32::try_from(destination.width().get()).unwrap_or(0);
    let destination_height = u32::try_from(destination.height().get()).unwrap_or(0);

    let Some(sampler) = ImageSampler::new(
        image,
        source_width,
        source_height,
        destination_width,
        destination_height,
        paint.sampling,
    ) else {
        return Ok(EInkTone::Binary);
    };

    let destination_x = destination.x().get();
    let destination_y = destination.y().get();

    let left = visible.x().get();
    let top = visible.y().get();
    let right = visible.right().get();
    let bottom = visible.bottom().get();

    let tone = Cell::new(EInkTone::Binary);
    let tone_ref = &tone;

    let pixels = (top..bottom).flat_map(|y| {
        let relative_y = i64::from(y) - i64::from(destination_y);

        let row = u32::try_from(relative_y)
            .ok()
            .and_then(|relative_y| sampler.row(relative_y));

        (left..right).filter_map(move |x| {
            let row = row?;

            let relative_x = i64::from(x) - i64::from(destination_x);

            let relative_x = u32::try_from(relative_x).ok()?;

            let color = row.sample(relative_x)?;

            let color = process_image_pixel(color, paint, x, y);

            let gray = color_to_gray2(color);

            if matches!(gray.luma(), 1 | 2) {
                tone_ref.set(EInkTone::Gray4);
            }

            Some(EgPixel(EgPoint::new(x, y), gray))
        })
    });

    target.draw_iter(pixels)?;

    Ok(tone.get())
}
