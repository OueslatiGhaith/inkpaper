use crate::{
    Display, Edges, FlexDirection, Length, Offset, Pixels, Point, Rect, ResolvedTextStyle, Size,
    Style, px,
};

mod boxes;
mod flex;
mod flow;
mod measure;
#[cfg(test)]
mod tests;

pub(crate) use self::boxes::{BorderBox, ContainingBlock, PaddingBox};

pub trait TextMeasurer {
    /// the size of `text` laid out within `available_width`, at whatever height its
    /// lines take. Layout clamps the height to the space it has
    fn measure_text(
        &self,
        text: &str,
        style: ResolvedTextStyle,
        available_width: AvailableSpace,
    ) -> Size;
}

/// the space a node may take along one axis
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum AvailableSpace {
    /// at most this much
    Definite(Pixels),
    /// as much as the content takes, such as along a scroll container's scrolling axis
    Unbounded,
}

impl AvailableSpace {
    pub const fn definite(self) -> Option<Pixels> {
        match self {
            Self::Definite(space) => Some(space),
            Self::Unbounded => None,
        }
    }

    /// `size`, cut down to fit the space
    pub const fn limit(self, size: Pixels) -> Pixels {
        match self {
            Self::Definite(space) => size.min(space.non_negative()),
            Self::Unbounded => size,
        }
    }

    /// the space left after `used`
    const fn shrink(self, used: Pixels) -> Self {
        match self {
            Self::Definite(space) => Self::Definite(space.saturating_sub(used).non_negative()),
            Self::Unbounded => Self::Unbounded,
        }
    }
}

/// the space a node may take along both axes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AvailableSize {
    pub(crate) width: AvailableSpace,
    pub(crate) height: AvailableSpace,
}

impl AvailableSize {
    pub(crate) const fn new(width: AvailableSpace, height: AvailableSpace) -> Self {
        Self { width, height }
    }

    pub(crate) const fn definite(size: Size) -> Self {
        Self::new(
            AvailableSpace::Definite(size.width),
            AvailableSpace::Definite(size.height),
        )
    }

    /// `size`, cut down to fit the space
    const fn limit(self, size: Size) -> Size {
        Size::new(self.width.limit(size.width), self.height.limit(size.height))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Axis {
    Horizontal,
    Vertical,
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

fn available_main(available: AvailableSize, axis: Axis) -> AvailableSpace {
    match axis {
        Axis::Horizontal => available.width,
        Axis::Vertical => available.height,
    }
}

fn available_cross(available: AvailableSize, axis: Axis) -> AvailableSpace {
    match axis {
        Axis::Horizontal => available.height,
        Axis::Vertical => available.width,
    }
}

fn available_from_axes(main: AvailableSpace, cross: AvailableSpace, axis: Axis) -> AvailableSize {
    match axis {
        Axis::Horizontal => AvailableSize::new(main, cross),
        Axis::Vertical => AvailableSize::new(cross, main),
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

fn content_size(style: Style, outer_size: Size) -> Size {
    Size::new(
        (outer_size.width - horizontal_chrome(style)).non_negative(),
        (outer_size.height - vertical_chrome(style)).non_negative(),
    )
}

fn content_available(style: Style, outer_available: AvailableSize) -> AvailableSize {
    AvailableSize::new(
        outer_available.width.shrink(horizontal_chrome(style)),
        outer_available.height.shrink(vertical_chrome(style)),
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

#[allow(clippy::too_many_arguments)]
fn absolute_axis_origin(
    containing_start: Pixels,
    containing_size: Pixels,
    content_start: Pixels,
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
        content_start + margin_start
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

/// `length` resolved in `available` space. Without a bound to fill, a filling length
/// takes its natural size
fn resolve_dimension(
    length: Length,
    minimum: Option<Pixels>,
    maximum: Option<Pixels>,
    available: AvailableSpace,
    natural: Pixels,
) -> Pixels {
    let base = match (length, available) {
        (Length::Pixels(value), _) => value.non_negative(),
        (Length::Fill, AvailableSpace::Definite(space)) => space.non_negative(),
        (Length::Auto | Length::Fill, _) => available.limit(natural.non_negative()),
    };

    clamp_to_limits(base, minimum, maximum)
}

/// `value` within a node's minimum and maximum. The minimum wins when they cross
fn clamp_to_limits(value: Pixels, minimum: Option<Pixels>, maximum: Option<Pixels>) -> Pixels {
    let minimum = minimum.map(Pixels::non_negative).unwrap_or(px(0));
    let value = match maximum {
        Some(maximum) => value.min(maximum.non_negative()),
        None => value,
    };

    value.max(minimum)
}

/// the space a node of `length` measures its content in
fn measurement_limit(
    length: Length,
    maximum: Option<Pixels>,
    available: AvailableSpace,
) -> AvailableSpace {
    let requested = match length {
        Length::Pixels(value) => AvailableSpace::Definite(value.non_negative()),
        Length::Auto | Length::Fill => available,
    };

    match (requested, maximum) {
        (requested, None) => requested,
        (AvailableSpace::Unbounded, Some(maximum)) => {
            AvailableSpace::Definite(maximum.non_negative())
        }
        (AvailableSpace::Definite(requested), Some(maximum)) => {
            AvailableSpace::Definite(requested.min(maximum.non_negative()))
        }
    }
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
    content: AvailableSize,
    margin: Edges<Pixels>,
    axis: Axis,
    allocated_main: Option<Pixels>,
) -> AvailableSize {
    let main = match allocated_main {
        Some(main) => AvailableSpace::Definite(main),
        None => available_main(content, axis).shrink(main_margin_total(margin, axis)),
    };
    let cross = available_cross(content, axis).shrink(cross_margin_total(margin, axis));

    available_from_axes(main, cross, axis)
}

fn size_with_main(size: Size, main: Pixels, axis: Axis) -> Size {
    match axis {
        Axis::Horizontal => Size::new(main, size.height),
        Axis::Vertical => Size::new(size.width, main),
    }
}
