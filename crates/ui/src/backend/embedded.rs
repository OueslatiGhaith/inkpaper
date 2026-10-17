//! Conversions between the ui's geometry and embedded-graphics', and box
//! drawing, shared by the embedded-graphics backends.

use embedded_graphics::{
    Drawable,
    geometry::{Point as EgPoint, Size as EgSize},
    pixelcolor::PixelColor,
    prelude::DrawTarget as EgDrawTarget,
    primitives::{
        Primitive, PrimitiveStyleBuilder as EgPrimitiveStyleBuilder, Rectangle as EgRectangle,
        RoundedRectangle as EgRoundedRectangle, StrokeAlignment as EgStrokeAlignment,
    },
};

use crate::{BoxPaint, Color, Point, Rect, Size, px};

/// Converts a ui rect to an embedded-graphics rectangle, treating negative sizes as
/// empty.
pub fn to_embedded_rect(rect: Rect) -> EgRectangle {
    let width = u32::try_from(rect.width().non_negative().get()).unwrap_or(u32::MAX);
    let height = u32::try_from(rect.height().non_negative().get()).unwrap_or(u32::MAX);

    EgRectangle::new(
        EgPoint::new(rect.x().get(), rect.y().get()),
        EgSize::new(width, height),
    )
}

/// Converts a ui point to an embedded-graphics point.
pub fn to_embedded_point(point: Point) -> EgPoint {
    EgPoint::new(point.x.get(), point.y.get())
}

/// Converts an embedded-graphics size to a ui size, saturating at `i32::MAX`.
pub fn from_embedded_size(size: EgSize) -> Size {
    Size::new(
        px(i32::try_from(size.width).unwrap_or(i32::MAX)),
        px(i32::try_from(size.height).unwrap_or(i32::MAX)),
    )
}

/// Draws a box's background and border, converting their colors with
/// `target_color`.
pub fn draw_box_to<D, C>(
    target: &mut D,
    bounds: Rect,
    paint: BoxPaint,
    target_color: fn(Color) -> C,
) -> Result<(), D::Error>
where
    D: EgDrawTarget<Color = C>,
    C: PixelColor,
{
    if bounds.width().is_non_positive() || bounds.height().is_non_positive() {
        return Ok(());
    }

    if paint.background.is_none() && paint.border.is_none() {
        return Ok(());
    }

    let mut style = EgPrimitiveStyleBuilder::new().stroke_alignment(EgStrokeAlignment::Inside);

    if let Some(background) = paint.background {
        style = style.fill_color(target_color(background));
    }

    if let Some(border) = paint.border {
        let width = u32::try_from(border.width.non_negative().get()).unwrap_or(u32::MAX);

        if width > 0 {
            style = style
                .stroke_color(target_color(border.color))
                .stroke_width(width);
        }
    }

    let style = style.build();

    let rectangle = to_embedded_rect(bounds);

    let radius = u32::try_from(paint.radius.non_negative().get()).unwrap_or(u32::MAX);

    if radius == 0 {
        rectangle.into_styled(style).draw(target)?;
    } else {
        EgRoundedRectangle::with_equal_corners(rectangle, EgSize::new(radius, radius))
            .into_styled(style)
            .draw(target)?;
    }

    Ok(())
}
