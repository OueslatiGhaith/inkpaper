//! Fits, samples and processes images into rows of final colors, leaving only
//! putting them down to a [`PixelSink`].

use crate::{
    Color, ImagePaint, ImageResource, ImageSampler, Point, Rect, fitted_image_bounds,
    process_image_pixel, px,
};

/// how many pixels of a row are handed to a [`PixelSink`] at once
const SPAN_CAPACITY: usize = 64;

/// where painted images end up: horizontal spans of final colors
pub(crate) trait PixelSink {
    type Error;

    /// draws `pixels` left to right, starting at `origin`
    fn draw_pixels(&mut self, origin: Point, pixels: &[Color]) -> Result<(), Self::Error>;
}

/// paints `image` fitted into `bounds`, only touching pixels inside `clip`
pub(crate) fn paint_image<S>(
    sink: &mut S,
    image: &dyn ImageResource,
    bounds: Rect,
    paint: ImagePaint,
    clip: Option<Rect>,
) -> Result<(), S::Error>
where
    S: PixelSink,
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

    let Some(sampler) = ImageSampler::new(
        image,
        source_width,
        source_height,
        destination_width,
        destination_height,
        paint.sampling,
    ) else {
        return Ok(());
    };

    let destination_x = destination.x().get();
    let destination_y = destination.y().get();

    let left = visible.x().get();
    let top = visible.y().get();
    let right = visible.right().get();
    let bottom = visible.bottom().get();

    let relative_left = i64::from(left) - i64::from(destination_x);
    let Some(relative_left) = u32::try_from(relative_left).ok() else {
        return Ok(());
    };

    let mut span = Span::new();

    for y in top..bottom {
        let relative_y = i64::from(y) - i64::from(destination_y);

        let Some(mut cursor) = u32::try_from(relative_y)
            .ok()
            .and_then(|relative_y| sampler.row(relative_y))
            .and_then(|row| row.cursor(relative_left))
        else {
            continue;
        };

        for x in left..right {
            match cursor.sample_next() {
                Some(color) => span.push(sink, x, y, process_image_pixel(color, paint, x, y))?,
                // a pixel the sampler can't produce stays unpainted
                None => span.flush(sink)?,
            }
        }

        span.flush(sink)?;
    }

    Ok(())
}

/// consecutive pixels of one row waiting to be drawn
struct Span {
    origin: Point,
    pixels: [Color; SPAN_CAPACITY],
    len: usize,
}

impl Span {
    const fn new() -> Self {
        Self {
            origin: Point::ZERO,
            pixels: [Color::BLACK; SPAN_CAPACITY],
            len: 0,
        }
    }

    fn push<S: PixelSink>(
        &mut self,
        sink: &mut S,
        x: i32,
        y: i32,
        color: Color,
    ) -> Result<(), S::Error> {
        if self.len == 0 {
            self.origin = Point::new(px(x), px(y));
        }

        self.pixels[self.len] = color;
        self.len += 1;

        if self.len == SPAN_CAPACITY {
            self.flush(sink)?;
        }

        Ok(())
    }

    fn flush<S: PixelSink>(&mut self, sink: &mut S) -> Result<(), S::Error> {
        if self.len == 0 {
            return Ok(());
        }

        let len = self.len;
        self.len = 0;

        sink.draw_pixels(self.origin, &self.pixels[..len])
    }
}
