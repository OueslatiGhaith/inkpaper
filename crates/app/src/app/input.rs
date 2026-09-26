use inkpaper_ui::prelude::*;

use super::{InkPaperApp, Screen};
use crate::{
    control_center::{ControlCenterAction, ControlCenterPointerResult, ControlCenterSlider},
    input::PointerAction,
};

impl InkPaperApp {
    pub(crate) fn handle_previous_input(&mut self, cx: &mut Context<'_, Self>) -> bool {
        if self.file_transfer_blocks_input() {
            return true;
        }

        if self.control_center.is_open() {
            // CrossInk uses +/- 5 for the physical side buttons.
            if self.frontlight.adjust_brightness(-5) {
                cx.notify();
            }

            return true;
        }

        self.reader_previous_page(cx)
    }

    pub(crate) fn handle_next_input(&mut self, cx: &mut Context<'_, Self>) -> bool {
        if self.file_transfer_blocks_input() {
            return true;
        }

        if self.control_center.is_open() {
            if self.frontlight.adjust_brightness(5) {
                cx.notify();
            }

            return true;
        }

        self.reader_next_page(cx)
    }

    pub(crate) fn handle_home_input(&mut self, cx: &mut Context<'_, Self>) {
        if self.file_transfer_blocks_input() {
            return;
        }

        if self.control_center.close() {
            self.frontlight.request_persist();
            cx.notify();

            return;
        }

        self.navigate_home(cx);
    }

    pub(crate) fn allows_focus_navigation(&self) -> bool {
        !self.control_center.is_open()
            && !(self.screen() == Screen::FileTransfer && self.file_transfer.blocks_input())
    }

    pub(crate) fn allows_wheel_scroll(&self) -> bool {
        !self.control_center.is_open()
            && !(self.screen() == Screen::FileTransfer && self.file_transfer.blocks_input())
    }

    pub(crate) fn handle_confirm_down(&self) -> bool {
        self.control_center.is_open()
            || (self.screen() == Screen::FileTransfer && self.file_transfer.blocks_input())
    }

    pub(crate) fn handle_confirm_up(&mut self, cx: &mut Context<'_, Self>) -> bool {
        if self.file_transfer_blocks_input() {
            return true;
        }

        if !self.control_center.is_open() {
            return false;
        }

        if self.frontlight.toggle() {
            cx.notify();
        }

        true
    }

    pub(crate) fn handle_pointer_down(
        &mut self,
        position: Point,
        cx: &mut Context<'_, Self>,
    ) -> PointerAction {
        if self.file_transfer_blocks_input() {
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
        if self.file_transfer_blocks_input() {
            return PointerAction::Capture;
        }

        let result = self.control_center.pointer_drag(origin, position);

        self.apply_control_center_pointer_result(result, PointerAction::Scroll, cx)
    }

    pub(crate) fn handle_pointer_up(
        &mut self,
        position: Point,
        cx: &mut Context<'_, Self>,
    ) -> PointerAction {
        if self.file_transfer_blocks_input() {
            return PointerAction::Capture;
        }

        let result = self.control_center.pointer_up(position);

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
