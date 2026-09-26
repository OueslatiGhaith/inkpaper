use inkpaper_ui::prelude::*;

use super::InkPaperApp;
use crate::{
    control_center::{ControlCenterAction, ControlCenterPointerResult, ControlCenterSlider},
    input::PointerAction,
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

    /// Handles a side button press. Unhandled presses move focus instead.
    fn side_button(
        &self,
        _app: &mut InkPaperApp,
        _button: SideButton,
        _cx: &mut Context<'_, InkPaperApp>,
    ) -> bool {
        false
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

    pub(crate) fn handle_previous_input(&mut self, cx: &mut Context<'_, Self>) -> bool {
        self.handle_side_button(SideButton::Previous, cx)
    }

    pub(crate) fn handle_next_input(&mut self, cx: &mut Context<'_, Self>) -> bool {
        self.handle_side_button(SideButton::Next, cx)
    }

    fn handle_side_button(&mut self, button: SideButton, cx: &mut Context<'_, Self>) -> bool {
        match self.input_target() {
            InputTarget::Blocked => true,

            InputTarget::ControlCenter => {
                let delta = match button {
                    SideButton::Previous => -5,
                    SideButton::Next => 5,
                };

                if self.frontlight.adjust_brightness(delta) {
                    cx.notify();
                }

                true
            }

            InputTarget::Screen => self.screen().route().side_button(self, button, cx),
        }
    }

    pub(crate) fn handle_home_input(&mut self, cx: &mut Context<'_, Self>) {
        match self.input_target() {
            InputTarget::Blocked => {}

            InputTarget::ControlCenter => {
                self.control_center.close();
                self.frontlight.request_persist();
                cx.notify();
            }

            InputTarget::Screen => self.navigate_home(cx),
        }
    }

    pub(crate) fn allows_focus_navigation(&self) -> bool {
        self.input_target() == InputTarget::Screen
    }

    pub(crate) fn allows_wheel_scroll(&self) -> bool {
        self.input_target() == InputTarget::Screen
    }

    pub(crate) fn handle_confirm_down(&self) -> bool {
        self.input_target() != InputTarget::Screen
    }

    pub(crate) fn handle_confirm_up(&mut self, cx: &mut Context<'_, Self>) -> bool {
        match self.input_target() {
            InputTarget::Blocked => true,

            InputTarget::ControlCenter => {
                if self.frontlight.toggle() {
                    cx.notify();
                }

                true
            }

            InputTarget::Screen => false,
        }
    }

    pub(crate) fn handle_pointer_down(
        &mut self,
        position: Point,
        cx: &mut Context<'_, Self>,
    ) -> PointerAction {
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
        if self.input_target() == InputTarget::Blocked {
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
        if self.input_target() == InputTarget::Blocked {
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

    pub(crate) fn cancel_pointer_input(&mut self) {
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
                self.frontlight.request_persist();
                cx.notify();
                PointerAction::Capture
            }

            ControlCenterPointerResult::Slider { slider, value } => {
                let changed = match slider {
                    ControlCenterSlider::Brightness => self.frontlight.set_brightness(value),
                    ControlCenterSlider::Warmth => self.frontlight.set_warmth(value),
                };

                if changed {
                    cx.notify();
                }

                PointerAction::Capture
            }

            ControlCenterPointerResult::Action(action) => {
                let changed = match action {
                    ControlCenterAction::AdjustBrightness(delta) => {
                        self.frontlight.adjust_brightness(delta)
                    }

                    ControlCenterAction::AdjustWarmth(delta) => {
                        self.frontlight.adjust_warmth(delta)
                    }

                    ControlCenterAction::ToggleFrontlight => self.frontlight.toggle(),
                };

                if changed {
                    cx.notify();
                }

                PointerAction::Capture
            }
        }
    }
}
