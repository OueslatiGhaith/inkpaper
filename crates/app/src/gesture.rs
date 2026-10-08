use inkpaper_ui::Point;

use crate::AppInputEvent;

/// Movement past this, in either direction, turns a touch into a drag.
const DRAG_THRESHOLD_PX: i32 = 12;

/// How long a touch must stay still to become a long press.
pub const LONG_PRESS_MS: u64 = 500;

/// Turns raw touch samples into pointer input. Platforms report where the
/// finger is and when, and call [`Self::tick`] while it's down so a still
/// finger can become a long press.
#[derive(Debug, Default)]
pub struct TouchGesture {
    active: Option<ActiveTouch>,
}

#[derive(Debug, Clone, Copy)]
struct ActiveTouch {
    origin: Point,
    position: Point,
    pressed_at_ms: u64,
    dragging: bool,
    long_pressed: bool,
}

impl TouchGesture {
    pub fn is_touching(&self) -> bool {
        self.active.is_some()
    }

    /// The finger is at `position`. The first sample of a touch presses the
    /// pointer; later ones may drag it.
    pub fn touch(&mut self, position: Point, now_ms: u64) -> Option<AppInputEvent> {
        let Some(active) = self.active.as_mut() else {
            self.active = Some(ActiveTouch {
                origin: position,
                position,
                pressed_at_ms: now_ms,
                dragging: false,
                long_pressed: false,
            });

            return Some(AppInputEvent::PointerDown(position));
        };

        if active.position == position {
            return None;
        }

        active.position = position;

        if active.dragging {
            return Some(AppInputEvent::PointerDrag {
                origin: active.origin,
                position,
            });
        }

        let origin = active.origin;
        if (origin.x.get() - position.x.get()).abs() < DRAG_THRESHOLD_PX
            && (origin.y.get() - position.y.get()).abs() < DRAG_THRESHOLD_PX
        {
            return None;
        }

        active.dragging = true;

        Some(AppInputEvent::PointerDrag { origin, position })
    }

    /// The finger has been held still long enough, once per touch.
    pub fn tick(&mut self, now_ms: u64) -> Option<AppInputEvent> {
        let active = self.active.as_mut()?;

        if active.dragging
            || active.long_pressed
            || now_ms.saturating_sub(active.pressed_at_ms) < LONG_PRESS_MS
        {
            return None;
        }

        active.long_pressed = true;

        Some(AppInputEvent::PointerLongPress(active.position))
    }

    /// The finger was lifted.
    pub fn release(&mut self) -> Option<AppInputEvent> {
        let active = self.active.take()?;

        Some(AppInputEvent::PointerUp(active.position))
    }
}

#[cfg(test)]
mod tests {
    use inkpaper_ui::px;

    use super::*;

    fn point(x: i32, y: i32) -> Point {
        Point::new(px(x), px(y))
    }

    #[test]
    fn a_still_touch_long_presses_once_after_the_delay() {
        let mut gesture = TouchGesture::default();

        gesture.touch(point(100, 100), 1_000);
        assert_eq!(gesture.touch(point(104, 98), 1_200), None);
        assert_eq!(gesture.tick(1_000 + LONG_PRESS_MS - 1), None);

        assert_eq!(
            gesture.tick(1_000 + LONG_PRESS_MS),
            Some(AppInputEvent::PointerLongPress(point(104, 98)))
        );
        assert_eq!(gesture.tick(5_000), None);
        assert_eq!(
            gesture.release(),
            Some(AppInputEvent::PointerUp(point(104, 98)))
        );
    }

    #[test]
    fn a_drag_never_long_presses() {
        let mut gesture = TouchGesture::default();

        gesture.touch(point(100, 100), 0);

        assert_eq!(
            gesture.touch(point(100, 112), 100),
            Some(AppInputEvent::PointerDrag {
                origin: point(100, 100),
                position: point(100, 112),
            })
        );
        assert_eq!(gesture.touch(point(100, 112), 200), None);
        assert_eq!(gesture.tick(LONG_PRESS_MS * 2), None);
    }

    #[test]
    fn each_touch_times_its_own_long_press() {
        let mut gesture = TouchGesture::default();

        gesture.touch(point(10, 10), 0);
        gesture.release();
        assert_eq!(gesture.tick(LONG_PRESS_MS), None);

        gesture.touch(point(10, 10), 400);
        assert_eq!(gesture.tick(LONG_PRESS_MS), None);
        assert!(gesture.tick(400 + LONG_PRESS_MS).is_some());
    }
}
