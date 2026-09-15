use embedded_graphics::geometry::Point as EgPoint;
use embedded_graphics_simulator::sdl2::MouseWheelDirection;
use inkpaper_ui::prelude::*;

const POINTER_DRAG_THRESHOLD_PX: u32 = 12;
const WHEEL_SCROLL_STEP_PX: i32 = 64;

#[derive(Debug, Default)]
pub struct PointerGesture {
    origin: Option<Point>,
    previous: Option<Point>,
    dragging: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct DragUpdate {
    pub origin: Point,
    pub delta: Offset,
    pub started: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum PointerRelease {
    None,
    Tap(Point),
    Drag { origin: Point, delta: Offset },
}

impl PointerGesture {
    pub fn begin(&mut self, position: Point) {
        self.origin = Some(position);
        self.previous = Some(position);
        self.dragging = false;
    }

    pub fn move_to(&mut self, position: Point) -> Option<DragUpdate> {
        let origin = self.origin?;
        let previous = self.previous.replace(position).unwrap_or(origin);

        if self.dragging {
            return Some(DragUpdate {
                origin,
                delta: previous - position,
                started: false,
            });
        }

        if !pointer_drag_threshold_reached(origin, position) {
            return None;
        }

        self.dragging = true;

        Some(DragUpdate {
            origin,
            delta: origin - position,
            started: true,
        })
    }

    pub fn finish(&mut self, position: Point) -> PointerRelease {
        let Some(origin) = self.origin.take() else {
            self.previous = None;
            self.dragging = false;
            return PointerRelease::None;
        };

        let previous = self.previous.take().unwrap_or(origin);
        let was_dragging = self.dragging;

        self.dragging = false;

        if was_dragging {
            return PointerRelease::Drag {
                origin,
                delta: previous - position,
            };
        }

        if pointer_drag_threshold_reached(origin, position) {
            return PointerRelease::Drag {
                origin,
                delta: origin - position,
            };
        }

        PointerRelease::Tap(position)
    }
}

fn pointer_drag_threshold_reached(origin: Point, position: Point) -> bool {
    origin.x.get().abs_diff(position.x.get()) >= POINTER_DRAG_THRESHOLD_PX
        || origin.y.get().abs_diff(position.y.get()) >= POINTER_DRAG_THRESHOLD_PX
}

pub fn wheel_scroll_offset(scroll_delta: EgPoint, direction: MouseWheelDirection) -> Offset {
    let vertical_steps = match direction {
        MouseWheelDirection::Flipped => scroll_delta.y,
        MouseWheelDirection::Normal | MouseWheelDirection::Unknown(_) => {
            scroll_delta.y.saturating_mul(-1)
        }
    };

    Offset::new(
        px(0),
        px(vertical_steps.saturating_mul(WHEEL_SCROLL_STEP_PX)),
    )
}
