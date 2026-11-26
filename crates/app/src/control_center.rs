use inkpaper_ui::prelude::*;

/// Minimum vertical travel (px) for edge swipe-down open and handle swipe-up close.
const SWIPE_THRESHOLD: i32 = 12;
/// Top bezel band (px) where a downward swipe opens the control center.
const TOP_EDGE_HEIGHT: i32 = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ControlCenterPointerResult {
    Pass,
    Activate,
    Capture,
    Opened,
    Closed,
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct ControlCenterState {
    open: bool,
    handle_drag_origin: Option<Point>,
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
        self.handle_drag_origin = None;
        self.opened_during_pointer = false;
    }

    pub(crate) fn pointer_down(
        &mut self,
        position: Point,
        on_handle: bool,
    ) -> ControlCenterPointerResult {
        if !self.open {
            return ControlCenterPointerResult::Pass;
        }

        self.handle_drag_origin = on_handle.then_some(position);
        self.opened_during_pointer = false;

        // Let the UI hit-test control center buttons, handle, backdrop, and sheet.
        ControlCenterPointerResult::Activate
    }

    pub(crate) fn pointer_drag(
        &mut self,
        origin: Point,
        position: Point,
    ) -> ControlCenterPointerResult {
        if !self.open {
            // The host only tells us that a drag happened. Whether that drag
            // opens the drawer or scrolls belongs here in the application.
            if origin.y.get() <= TOP_EDGE_HEIGHT
                && position.y.get() - origin.y.get() >= SWIPE_THRESHOLD
            {
                self.open = true;
                self.handle_drag_origin = None;
                self.opened_during_pointer = true;

                return ControlCenterPointerResult::Opened;
            }

            return ControlCenterPointerResult::Pass;
        }

        ControlCenterPointerResult::Capture
    }

    pub(crate) fn pointer_up(&mut self, position: Point) -> ControlCenterPointerResult {
        let handle_drag_origin = self.handle_drag_origin.take();
        let opened_during_pointer = core::mem::take(&mut self.opened_during_pointer);

        if !self.open {
            return ControlCenterPointerResult::Pass;
        }

        // Do not immediately dismiss a drawer that was just opened by this
        // same downward swipe, even if the finger finishes below the sheet.
        if opened_during_pointer {
            return ControlCenterPointerResult::Capture;
        }

        if let Some(down) = handle_drag_origin
            && down.y.get() - position.y.get() >= SWIPE_THRESHOLD
        {
            self.open = false;
            return ControlCenterPointerResult::Closed;
        }

        ControlCenterPointerResult::Activate
    }
}
