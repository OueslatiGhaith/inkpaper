use inkpaper_ui::prelude::*;

use super::InkPaperApp;
use crate::{
    control_center::{ControlCenterAction, ControlCenterPointerResult, ControlCenterSlider},
    input::{LongPressAction, PointerAction},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SideButton {
    Previous,
    Next,
}

/// Input behavior a screen defines for itself.
///
/// Overlays such as the control center take input before the screen sees it.
pub(crate) trait ScreenInput {
    /// Whether the screen currently swallows all input, including Home.
    fn blocks_input(&self, _app: &InkPaperApp) -> bool {
        false
    }

    /// Handles a side button press. Screens without one ignore it.
    fn side_button(
        &self,
        _app: &mut InkPaperApp,
        _button: SideButton,
        _cx: &mut Context<'_, InkPaperApp>,
    ) {
    }

    /// Handles the end of a touch no overlay claimed. Returning true captures
    /// it instead of activating the element under the pointer.
    fn release(
        &self,
        _app: &mut InkPaperApp,
        _position: Point,
        _cx: &mut Context<'_, InkPaperApp>,
    ) -> bool {
        false
    }

    /// Handles a touch held still that no overlay claimed. Returning true
    /// captures it instead of offering it to the element under the pointer.
    fn long_press(
        &self,
        _app: &mut InkPaperApp,
        _position: Point,
        _cx: &mut Context<'_, InkPaperApp>,
    ) -> bool {
        false
    }

    /// Handles a touch drag no overlay claimed. Returning true captures the
    /// drag instead of scrolling. Called for every move with the same origin.
    fn drag(
        &self,
        _app: &mut InkPaperApp,
        _origin: Point,
        _position: Point,
        _cx: &mut Context<'_, InkPaperApp>,
    ) -> bool {
        false
    }
}

/// Which layer receives input right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InputTarget {
    Blocked,
    ControlCenter,
    Screen,
}

impl InkPaperApp {
    fn input_target(&self) -> InputTarget {
        if self.screen().route().blocks_input(self) {
            InputTarget::Blocked
        } else if self.control_center.is_open() {
            InputTarget::ControlCenter
        } else {
            InputTarget::Screen
        }
    }

    pub(crate) fn handle_previous_input(&mut self, cx: &mut Context<'_, Self>) {
        self.handle_side_button(SideButton::Previous, cx);
    }

    pub(crate) fn handle_next_input(&mut self, cx: &mut Context<'_, Self>) {
        self.handle_side_button(SideButton::Next, cx);
    }

    fn handle_side_button(&mut self, button: SideButton, cx: &mut Context<'_, Self>) {
        match self.input_target() {
            InputTarget::Blocked => {}

            InputTarget::ControlCenter => {
                let delta = match button {
                    SideButton::Previous => -5,
                    SideButton::Next => 5,
                };

                self.change_frontlight(cx, |frontlight| frontlight.adjust_brightness(delta));
            }

            InputTarget::Screen => self.screen().route().side_button(self, button, cx),
        }
    }

    pub(crate) fn handle_home_input(&mut self, cx: &mut Context<'_, Self>) {
        match self.input_target() {
            InputTarget::Blocked => {}

            InputTarget::ControlCenter => {
                self.close_control_center(cx);
            }

            // like crosspoint's Back, it first returns from followed links
            InputTarget::Screen => {
                if self.screen() == super::Screen::Reader && self.reader.return_from_link() {
                    cx.notify();
                } else {
                    self.navigate_home(cx);
                }
            }
        }
    }

    pub(crate) fn allows_wheel_scroll(&self) -> bool {
        self.input_target() == InputTarget::Screen
    }

    pub(crate) fn handle_pointer_down(
        &mut self,
        position: Point,
        cx: &mut Context<'_, Self>,
    ) -> PointerAction {
        self.pointer_captured = false;

        if self.input_target() == InputTarget::Blocked {
            return PointerAction::Capture;
        }

        let result = self.control_center.pointer_down(position);

        self.apply_control_center_pointer_result(result, PointerAction::Activate, cx)
    }

    pub(crate) fn handle_pointer_drag(
        &mut self,
        origin: Point,
        position: Point,
        cx: &mut Context<'_, Self>,
    ) -> PointerAction {
        if self.pointer_captured || self.input_target() == InputTarget::Blocked {
            return PointerAction::Capture;
        }

        let result = self.control_center.pointer_drag(origin, position);

        if result == ControlCenterPointerResult::Pass
            && self.screen().route().drag(self, origin, position, cx)
        {
            return PointerAction::Capture;
        }

        self.apply_control_center_pointer_result(result, PointerAction::Scroll, cx)
    }

    pub(crate) fn handle_pointer_up(
        &mut self,
        position: Point,
        cx: &mut Context<'_, Self>,
    ) -> PointerAction {
        if core::mem::take(&mut self.pointer_captured)
            || self.input_target() == InputTarget::Blocked
        {
            return PointerAction::Capture;
        }

        let result = self.control_center.pointer_up(position);

        if result == ControlCenterPointerResult::Pass
            && self.screen().route().release(self, position, cx)
        {
            return PointerAction::Capture;
        }

        self.apply_control_center_pointer_result(result, PointerAction::Activate, cx)
    }

    pub(crate) fn handle_pointer_long_press(
        &mut self,
        position: Point,
        cx: &mut Context<'_, Self>,
    ) -> LongPressAction {
        if self.pointer_captured || self.input_target() != InputTarget::Screen {
            return LongPressAction::Ignore;
        }

        if self.screen().route().long_press(self, position, cx) {
            LongPressAction::Capture
        } else {
            LongPressAction::Dispatch
        }
    }

    /// The rest of the current touch, up to its release, is ignored.
    pub(crate) fn capture_pointer(&mut self) {
        self.pointer_captured = true;
    }

    pub(crate) fn cancel_pointer_input(&mut self) {
        self.pointer_captured = false;
        self.control_center.cancel_pointer();
    }

    fn apply_control_center_pointer_result(
        &mut self,
        result: ControlCenterPointerResult,
        pass: PointerAction,
        cx: &mut Context<'_, Self>,
    ) -> PointerAction {
        match result {
            ControlCenterPointerResult::Pass => pass,

            ControlCenterPointerResult::Capture => PointerAction::Capture,

            ControlCenterPointerResult::Opened => {
                cx.notify();
                PointerAction::Capture
            }

            ControlCenterPointerResult::Closed => {
                self.request_frontlight_persist(cx);
                cx.notify();
                PointerAction::Capture
            }

            ControlCenterPointerResult::Slider { slider, value } => {
                // only the control center's panel shows the setting
                self.change_frontlight(cx, |frontlight| match slider {
                    ControlCenterSlider::Brightness => frontlight.set_brightness(value),
                    ControlCenterSlider::Warmth => frontlight.set_warmth(value),
                });

                PointerAction::Capture
            }

            ControlCenterPointerResult::Action(action) => {
                self.change_frontlight(cx, |frontlight| match action {
                    ControlCenterAction::AdjustBrightness(delta) => {
                        frontlight.adjust_brightness(delta)
                    }

                    ControlCenterAction::AdjustWarmth(delta) => frontlight.adjust_warmth(delta),

                    ControlCenterAction::ToggleFrontlight => frontlight.toggle(),
                });

                PointerAction::Capture
            }
        }
    }
}
