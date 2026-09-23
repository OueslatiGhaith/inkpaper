use embedded_graphics_simulator::sdl2::MouseWheelDirection;
use inkpaper_ui::prelude::*;

const DRAG_THRESHOLD_PX: i32 = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DragUpdate {
    pub(crate) origin: Point,
    pub(crate) previous: Point,
    pub(crate) position: Point,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PointerRelease {
    None,
    Tap(Point),
    Drag {
        origin: Point,
        previous: Point,
        position: Point,
    },
}

#[derive(Debug, Default)]
pub(crate) struct PointerGesture {
    origin: Option<Point>,
    previous: Option<Point>,
    dragging: bool,
}

impl PointerGesture {
    pub(crate) fn begin(&mut self, position: Point) {
        self.origin = Some(position);
        self.previous = Some(position);
        self.dragging = false;
    }

    pub(crate) fn move_to(&mut self, position: Point) -> Option<DragUpdate> {
        let origin = self.origin?;

        if self.dragging {
            let previous = self.previous.unwrap_or(origin);
            self.previous = Some(position);

            if previous == position {
                return None;
            }

            return Some(DragUpdate {
                origin,
                previous,
                position,
            });
        }

        self.previous = Some(position);

        if !drag_threshold_reached(origin, position) {
            return None;
        }

        self.dragging = true;

        // Match the firmware: the first emitted update includes all movement
        // accumulated before the threshold was crossed.
        Some(DragUpdate {
            origin,
            previous: origin,
            position,
        })
    }

    pub(crate) fn finish(&mut self, position: Point) -> PointerRelease {
        let Some(origin) = self.origin.take() else {
            return PointerRelease::None;
        };

        let previous = self.previous.take().unwrap_or(origin);
        let was_dragging = self.dragging;

        self.dragging = false;

        if was_dragging || drag_threshold_reached(origin, position) {
            return PointerRelease::Drag {
                origin,
                previous: if was_dragging { previous } else { origin },
                position,
            };
        }

        PointerRelease::Tap(position)
    }
}

fn drag_threshold_reached(origin: Point, position: Point) -> bool {
    (origin.x.get() - position.x.get()).abs() >= DRAG_THRESHOLD_PX
        || (origin.y.get() - position.y.get()).abs() >= DRAG_THRESHOLD_PX
}

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
