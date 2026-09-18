use crate::{Color, ImageResource};

pub(super) fn sample_nearest(
    image: &dyn ImageResource,
    x: u32,
    y: u32,
    source_width: u32,
    source_height: u32,
    destination_width: u32,
    destination_height: u32,
) -> Option<Color> {
    let source_x =
        u64::from(x).saturating_mul(u64::from(source_width)) / u64::from(destination_width);
    let source_y =
        u64::from(y).saturating_mul(u64::from(source_height)) / u64::from(destination_height);

    let source_x = source_x.min(u64::from(source_width.saturating_sub(1)));
    let source_y = source_y.min(u64::from(source_height.saturating_sub(1)));

    image.pixel(u32::try_from(source_x).ok()?, u32::try_from(source_y).ok()?)
}
