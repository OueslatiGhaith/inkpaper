use crate::{Color, ImageResource};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct NearestRow {
    source_y: u32,
}

pub(super) fn prepare_row(
    y: u32,
    source_height: u32,
    destination_height: u32,
) -> Option<NearestRow> {
    Some(NearestRow {
        source_y: nearest_axis(y, source_height, destination_height)?,
    })
}

pub(super) fn sample_nearest(
    image: &dyn ImageResource,
    x: u32,
    source_width: u32,
    destination_width: u32,
    row: NearestRow,
) -> Option<Color> {
    let source_x = nearest_axis(x, source_width, destination_width)?;

    image.pixel(source_x, row.source_y)
}

fn nearest_axis(destination: u32, source_len: u32, destination_len: u32) -> Option<u32> {
    if source_len == 0 || destination_len == 0 || destination >= destination_len {
        return None;
    }

    let source =
        u64::from(destination).saturating_mul(u64::from(source_len)) / u64::from(destination_len);

    let source = source.min(u64::from(source_len - 1));

    u32::try_from(source).ok()
}
