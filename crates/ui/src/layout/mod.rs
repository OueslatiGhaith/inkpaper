use crate::{
    Display, Edges, FlexDirection, Length, Offset, Pixels, Point, Rect, ResolvedTextStyle, Size,
    Style, px,
};

mod flex;
mod flow;
mod measure;

pub trait TextMeasurer {
    fn measure_text(&self, text: &str, style: ResolvedTextStyle, max_size: Size) -> Size;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Axis {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy)]
struct FlexTotals {
    base_size: Pixels,
    grow_weight: u64,
    shrink_factor: u64,
}

fn main_size(size: Size, axis: Axis) -> Pixels {
    match axis {
        Axis::Horizontal => size.width,
        Axis::Vertical => size.height,
    }
}

fn cross_size(size: Size, axis: Axis) -> Pixels {
    match axis {
        Axis::Horizontal => size.height,
        Axis::Vertical => size.width,
    }
}

fn size_from_axes(main: Pixels, cross: Pixels, axis: Axis) -> Size {
    match axis {
        Axis::Horizontal => Size::new(main, cross),
        Axis::Vertical => Size::new(cross, main),
    }
}

fn point_from_axes(main: Pixels, cross: Pixels, axis: Axis) -> Point {
    match axis {
        Axis::Horizontal => Point::new(main, cross),
        Axis::Vertical => Point::new(cross, main),
    }
}

fn requested_length(style: Style, axis: Axis) -> Length {
    match axis {
        Axis::Horizontal => style.width,
        Axis::Vertical => style.height,
    }
}

pub(crate) fn flow_axis(style: Style) -> Axis {
    match style.display {
        Display::Block => Axis::Vertical,
        Display::Flex => match style.flex_direction {
            FlexDirection::Row => Axis::Horizontal,
            FlexDirection::Column => Axis::Vertical,
        },
    }
}

fn content_available(style: Style, outer_available: Size) -> Size {
    Size::new(
        (outer_available.width - horizontal_chrome(style)).non_negative(),
        (outer_available.height - vertical_chrome(style)).non_negative(),
    )
}

fn content_rect(style: Style, origin: Point, outer_size: Size) -> Rect {
    let border = border_width(style);

    Rect::new(
        Point::new(
            origin.x + border + style.padding.left.non_negative(),
            origin.y + border + style.padding.top.non_negative(),
        ),
        content_available(style, outer_size),
    )
}

fn absolute_axis_available(
    containing_size: Pixels,
    start: Option<Pixels>,
    end: Option<Pixels>,
    margin_start: Pixels,
    margin_end: Pixels,
) -> Pixels {
    (containing_size
        - start.unwrap_or(px(0))
        - end.unwrap_or(px(0))
        - margin_start.non_negative()
        - margin_end.non_negative())
    .non_negative()
}

fn absolute_axis_origin(
    containing_start: Pixels,
    containing_size: Pixels,
    child_size: Pixels,
    start: Option<Pixels>,
    end: Option<Pixels>,
    margin_start: Pixels,
    margin_end: Pixels,
) -> Pixels {
    let margin_start = margin_start.non_negative();
    let margin_end = margin_end.non_negative();

    if let Some(start) = start {
        containing_start + start + margin_start
    } else if let Some(end) = end {
        containing_start + containing_size - end - margin_end - child_size
    } else {
        containing_start + margin_start
    }
}

fn relative_axis_offset(start: Option<Pixels>, end: Option<Pixels>) -> Pixels {
    if let Some(start) = start {
        start
    } else if let Some(end) = end {
        px(0) - end
    } else {
        px(0)
    }
}

fn relative_offset(style: Style) -> Offset {
    Offset::new(
        relative_axis_offset(style.inset.left, style.inset.right),
        relative_axis_offset(style.inset.top, style.inset.bottom),
    )
}

fn border_width(style: Style) -> Pixels {
    style.border_width.non_negative()
}

fn horizontal_chrome(style: Style) -> Pixels {
    style.padding.left.non_negative() + style.padding.right.non_negative() + border_width(style) * 2
}

fn vertical_chrome(style: Style) -> Pixels {
    style.padding.top.non_negative() + style.padding.bottom.non_negative() + border_width(style) * 2
}

fn resolve_dimension(
    length: Length,
    minimum: Option<Pixels>,
    maximum: Option<Pixels>,
    available: Pixels,
    natural: Pixels,
) -> Pixels {
    let available = available.non_negative();
    let base = match length {
        Length::Auto => natural.non_negative().min(available),
        Length::Pixels(value) => value.non_negative(),
        Length::Fill => available,
    };

    let minimum = minimum.map(Pixels::non_negative).unwrap_or(px(0));
    let maximum = maximum
        .map(Pixels::non_negative)
        .unwrap_or(Pixels::MAX)
        .max(minimum);

    base.clamp(minimum, maximum)
}

fn measurement_limit(length: Length, maximum: Option<Pixels>, available: Pixels) -> Pixels {
    let available = available.non_negative();
    let requested = match length {
        Length::Auto => available,
        Length::Pixels(value) => value.non_negative(),
        Length::Fill => available,
    };

    let maximum = maximum.map(Pixels::non_negative).unwrap_or(Pixels::MAX);

    requested.min(maximum)
}

fn main_margin_start(margin: Edges<Pixels>, axis: Axis) -> Pixels {
    match axis {
        Axis::Horizontal => margin.left.non_negative(),
        Axis::Vertical => margin.top.non_negative(),
    }
}

fn main_margin_end(margin: Edges<Pixels>, axis: Axis) -> Pixels {
    match axis {
        Axis::Horizontal => margin.right.non_negative(),
        Axis::Vertical => margin.bottom.non_negative(),
    }
}

fn cross_margin_start(margin: Edges<Pixels>, axis: Axis) -> Pixels {
    match axis {
        Axis::Horizontal => margin.top.non_negative(),
        Axis::Vertical => margin.left.non_negative(),
    }
}

fn cross_margin_end(margin: Edges<Pixels>, axis: Axis) -> Pixels {
    match axis {
        Axis::Horizontal => margin.bottom.non_negative(),
        Axis::Vertical => margin.right.non_negative(),
    }
}

fn main_margin_total(margin: Edges<Pixels>, axis: Axis) -> Pixels {
    main_margin_start(margin, axis).saturating_add(main_margin_end(margin, axis))
}

fn cross_margin_total(margin: Edges<Pixels>, axis: Axis) -> Pixels {
    cross_margin_start(margin, axis).saturating_add(cross_margin_end(margin, axis))
}

fn child_constraint(
    content_size: Size,
    margin: Edges<Pixels>,
    axis: Axis,
    allocated_main: Option<Pixels>,
) -> Size {
    let main = allocated_main.unwrap_or_else(|| {
        (main_size(content_size, axis) - main_margin_total(margin, axis)).non_negative()
    });
    let cross = (cross_size(content_size, axis) - cross_margin_total(margin, axis)).non_negative();

    size_from_axes(main, cross, axis)
}

fn weighted_share(
    amount: Pixels,
    previous_weight: u64,
    next_weight: u64,
    total_weight: u64,
) -> Pixels {
    if amount.is_non_positive() || total_weight <= previous_weight {
        return px(0);
    }

    let amount = amount.get() as u64;
    let previous = amount.saturating_mul(previous_weight) / total_weight;
    let next = amount.saturating_mul(next_weight) / total_weight;
    let share = next.saturating_sub(previous);

    px(i32::try_from(share).unwrap_or(i32::MAX))
}

fn size_with_main(size: Size, main: Pixels, axis: Axis) -> Size {
    match axis {
        Axis::Horizontal => Size::new(main, size.height),
        Axis::Vertical => Size::new(size.width, main),
    }
}




#[cfg(test)]
mod tests;
