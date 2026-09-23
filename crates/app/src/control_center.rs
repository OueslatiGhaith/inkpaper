use inkpaper_ui::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ControlCenterSlider {
    Brightness,
    Warmth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ControlCenterAction {
    AdjustBrightness(i16),
    AdjustWarmth(i16),
    ToggleFrontlight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ControlCenterPointerResult {
    Pass,
    Capture,
    Opened,
    Closed,
    Slider {
        slider: ControlCenterSlider,
        value: u8,
    },
    Action(ControlCenterAction),
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct ControlCenterState {
    open: bool,
    pointer_down: Option<Point>,
    active_slider: Option<ControlCenterSlider>,
    pressed_action: Option<ControlCenterAction>,
    handle_armed: bool,
    opened_during_pointer: bool,
}

impl ControlCenterState {
    pub(crate) const fn is_open(&self) -> bool {
        self.open
    }

    pub(crate) fn close(&mut self) -> bool {
        if !self.open {
            return false;
        }

        self.open = false;
        self.cancel_pointer();
        true
    }

    pub(crate) fn cancel_pointer(&mut self) {
        self.pointer_down = None;
        self.active_slider = None;
        self.pressed_action = None;
        self.handle_armed = false;
        self.opened_during_pointer = false;
    }

    pub(crate) fn pointer_down(&mut self, position: Point) -> ControlCenterPointerResult {
        if !self.open {
            return ControlCenterPointerResult::Pass;
        }

        self.pointer_down = Some(position);
        self.active_slider = None;
        self.pressed_action = None;
        self.handle_armed = false;
        self.opened_during_pointer = false;

        if in_rect(position, 92, 165, 296, 56) {
            self.active_slider = Some(ControlCenterSlider::Brightness);

            return ControlCenterPointerResult::Slider {
                slider: ControlCenterSlider::Brightness,
                value: brightness_from_x(position.x.get()),
            };
        }

        if in_rect(position, 92, 265, 296, 56) {
            self.active_slider = Some(ControlCenterSlider::Warmth);

            return ControlCenterPointerResult::Slider {
                slider: ControlCenterSlider::Warmth,
                value: warmth_from_x(position.x.get()),
            };
        }

        self.pressed_action = action_at(position);

        // CrossInk's drawer handle occupies the final sheet band.
        if in_rect(position, 0, 353, 480, 29) {
            self.handle_armed = true;
        }

        ControlCenterPointerResult::Capture
    }

    pub(crate) fn pointer_drag(
        &mut self,
        origin: Point,
        position: Point,
    ) -> ControlCenterPointerResult {
        if !self.open {
            // The host only tells us that a drag happened. Whether that drag
            // opens the drawer or scrolls belongs here in the application.
            if origin.y.get() <= 16 && position.y.get() - origin.y.get() >= 12 {
                self.open = true;
                self.pointer_down = Some(origin);
                self.opened_during_pointer = true;

                return ControlCenterPointerResult::Opened;
            }

            return ControlCenterPointerResult::Pass;
        }

        if let Some(slider) = self.active_slider {
            let value = match slider {
                ControlCenterSlider::Brightness => brightness_from_x(position.x.get()),
                ControlCenterSlider::Warmth => warmth_from_x(position.x.get()),
            };

            return ControlCenterPointerResult::Slider { slider, value };
        }

        // Once a press turns into a drag it is no longer a +/- or toggle tap.
        self.pressed_action = None;

        ControlCenterPointerResult::Capture
    }

    pub(crate) fn pointer_up(&mut self, position: Point) -> ControlCenterPointerResult {
        let pointer_down = self.pointer_down.take();
        let active_slider = self.active_slider.take();
        let pressed_action = self.pressed_action.take();
        let handle_armed = self.handle_armed;
        let opened_during_pointer = self.opened_during_pointer;

        self.handle_armed = false;
        self.opened_during_pointer = false;

        if let Some(slider) = active_slider {
            let value = match slider {
                ControlCenterSlider::Brightness => brightness_from_x(position.x.get()),
                ControlCenterSlider::Warmth => warmth_from_x(position.x.get()),
            };

            return ControlCenterPointerResult::Slider { slider, value };
        }

        if !self.open {
            return ControlCenterPointerResult::Pass;
        }

        // Do not immediately dismiss a drawer that was just opened by this
        // same downward swipe, even if the finger finishes below the sheet.
        if opened_during_pointer {
            return ControlCenterPointerResult::Capture;
        }

        // CrossInk dismisses when touching outside the top sheet.
        if position.y.get() >= 382 {
            self.open = false;
            return ControlCenterPointerResult::Closed;
        }

        if handle_armed {
            let tapped_handle = in_rect(position, 0, 353, 480, 29);

            let swiped_up = pointer_down
                .map(|down| down.y.get() - position.y.get() >= 12)
                .unwrap_or(false);

            if tapped_handle || swiped_up {
                self.open = false;
                return ControlCenterPointerResult::Closed;
            }
        }

        if let Some(action) = pressed_action {
            if action_at(position) == Some(action) {
                return ControlCenterPointerResult::Action(action);
            }
        }

        ControlCenterPointerResult::Capture
    }
}

fn action_at(position: Point) -> Option<ControlCenterAction> {
    // CrossInk's lightbulb hit zone extends into the right screen edge.
    if in_rect(position, 356, 101, 124, 64) {
        return Some(ControlCenterAction::ToggleFrontlight);
    }

    if in_rect(position, 32, 165, 56, 56) {
        return Some(ControlCenterAction::AdjustBrightness(-1));
    }

    if in_rect(position, 392, 165, 56, 56) {
        return Some(ControlCenterAction::AdjustBrightness(1));
    }

    if in_rect(position, 32, 265, 56, 56) {
        return Some(ControlCenterAction::AdjustWarmth(-1));
    }

    if in_rect(position, 392, 265, 56, 56) {
        return Some(ControlCenterAction::AdjustWarmth(1));
    }

    None
}

fn brightness_from_x(x: i32) -> u8 {
    let relative = (x - 92).clamp(0, 296);
    let value = (relative * 100 + 148) / 296;

    value.clamp(1, 100) as u8
}

fn warmth_from_x(x: i32) -> u8 {
    let relative = (x - 92).clamp(0, 296);
    ((relative * 100 + 148) / 296).clamp(0, 100) as u8
}

fn in_rect(point: Point, x: i32, y: i32, width: i32, height: i32) -> bool {
    let px = point.x.get();
    let py = point.y.get();

    px >= x && px < x + width && py >= y && py < y + height
}
