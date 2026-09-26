use inkpaper_ui::prelude::*;

use super::{InkPaperApp, Screen, navigation::ScreenLifecycle};
use crate::{
    BatteryStatus, ClockStatus, FrontlightPreferences, FrontlightPreferencesRequest,
    FrontlightSetting,
};

impl InkPaperApp {
    pub fn apply_battery_status(&mut self, battery: BatteryStatus, cx: &mut Context<'_, Self>) {
        let visible_changed = self.system_status.set_battery(battery);

        if visible_changed && (self.screen() != Screen::Reader || self.control_center.is_open()) {
            cx.notify();
        }
    }

    pub fn apply_clock_status(&mut self, clock: Option<ClockStatus>, cx: &mut Context<'_, Self>) {
        let changed = self.system_status.set_clock(clock);

        // The clock is currently rendered only by Settings.
        //
        // Keeping RTC updates out of the Reader avoids waking the e-ink display
        // once per minute while somebody is reading.
        if changed && (self.screen() == Screen::Settings || self.control_center.is_open()) {
            cx.notify();
        }
    }

    pub(crate) fn apply_frontlight_preferences(
        &mut self,
        preferences: FrontlightPreferences,
        cx: &mut Context<'_, Self>,
    ) {
        if self.frontlight.apply_preferences(preferences) {
            cx.notify();
        }
    }

    pub fn request_frontlight_apply(&mut self) {
        self.frontlight.request_apply();
    }

    pub(crate) fn take_frontlight_request(&mut self) -> Option<FrontlightSetting> {
        self.frontlight.take_request()
    }

    pub(crate) fn take_frontlight_preferences_request(
        &mut self,
    ) -> Option<FrontlightPreferencesRequest> {
        self.frontlight.take_preferences_request()
    }

    /// Whether the device may enter deep sleep on its own after inactivity.
    ///
    /// A USB Drive session hands the SD card to the host, so sleeping would cut
    /// off a transfer in progress.
    pub fn allows_auto_sleep(&self) -> bool {
        !self.file_transfer.blocks_input()
    }

    pub fn prepare_for_sleep(&mut self, cx: &mut Context<'_, Self>) {
        if self.control_center.close() {
            self.frontlight.request_persist();
        }

        if self.sleeping {
            return;
        }

        self.sleeping = true;
        cx.notify();
    }
}

pub(super) struct SettingsRoute;

impl ScreenLifecycle for SettingsRoute {}
