use embedded_graphics_simulator::sdl2::MouseWheelDirection;
use inkpaper_app::AppInputEvent;

/// Lists turn pages rather than scroll, so the wheel stands in for the side
/// buttons, turning toward the end of a list the way it used to scroll.
pub(crate) fn wheel_input(
    delta: embedded_graphics::geometry::Point,
    direction: MouseWheelDirection,
) -> Option<AppInputEvent> {
    let delta = match direction {
        MouseWheelDirection::Flipped => -delta.y,
        MouseWheelDirection::Normal | MouseWheelDirection::Unknown(_) => delta.y,
    };

    match delta.signum() {
        1 => Some(AppInputEvent::Next),
        -1 => Some(AppInputEvent::Previous),
        _ => None,
    }
}
