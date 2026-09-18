use crate::{Color, ImageResource};

use super::axis::LinearQuotient;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct NearestRow {
    source_y: u32,
}

pub(super) struct NearestCursor {
    source_x: LinearQuotient,
    source_width: u32,
    remaining: u32,
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

pub(super) fn prepare_cursor(
    start_x: u32,
    source_width: u32,
    destination_width: u32,
    _row: NearestRow,
) -> Option<NearestCursor> {
    if source_width == 0 || destination_width == 0 || start_x >= destination_width {
        return None;
    }

    let initial_numerator = u128::from(start_x) * u128::from(source_width);

    let source_x = LinearQuotient::new(
        initial_numerator,
        u64::from(source_width),
        u64::from(destination_width),
    )?;

    Some(NearestCursor {
        source_x,
        source_width,
        remaining: destination_width - start_x,
    })
}

pub(super) fn sample_next(
    image: &dyn ImageResource,
    row: NearestRow,
    cursor: &mut NearestCursor,
) -> Option<Color> {
    if cursor.remaining == 0 {
        return None;
    }

    let source_x = cursor
        .source_x
        .quotient()
        .min(u64::from(cursor.source_width - 1));

    cursor.remaining -= 1;

    if cursor.remaining > 0 {
        cursor.source_x.advance();
    }

    image.pixel(u32::try_from(source_x).ok()?, row.source_y)
}

fn nearest_axis(destination: u32, source_len: u32, destination_len: u32) -> Option<u32> {
    if source_len == 0 || destination_len == 0 || destination >= destination_len {
        return None;
    }

    let source = u64::from(destination) * u64::from(source_len) / u64::from(destination_len);
    let source = source.min(u64::from(source_len - 1));

    u32::try_from(source).ok()
}
