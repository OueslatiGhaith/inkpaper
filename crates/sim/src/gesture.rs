use embedded_graphics_simulator::sdl2::MouseWheelDirection;
use inkpaper_ui::prelude::*;

pub(crate) fn wheel_scroll_offset(
    delta: embedded_graphics::geometry::Point,
    direction: MouseWheelDirection,
) -> Offset {
    let multiplier = match direction {
        MouseWheelDirection::Normal => 1,
        MouseWheelDirection::Flipped => -1,
        MouseWheelDirection::Unknown(_) => 1,
    };

    Offset::new(px(delta.x * multiplier * 24), px(delta.y * multiplier * 24))
}
