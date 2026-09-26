use inkpaper_ui::prelude::*;

use super::{InkPaperApp, Screen};
use crate::{
    BatteryStatus, ClockPreferences, ClockStatus, FrontlightPreferences,
    FrontlightPreferencesRequest, FrontlightSetting, UtcOffset,
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

    /// The RTC's UTC time shifted to the chosen timezone.
    pub(crate) fn local_clock(&self) -> Option<ClockStatus> {
        let minutes = self.clock.utc_offset().minutes();

        self.system_status
            .clock()
            .map(|clock| clock.offset_by(minutes))
    }

    pub(crate) fn apply_clock_preferences(
        &mut self,
        preferences: ClockPreferences,
        cx: &mut Context<'_, Self>,
    ) {
        if self.clock.apply_preferences(preferences) {
            cx.notify();
        }
    }

    /// Shows a timezone while its slider is dragged; release saves it.
    pub(crate) fn preview_utc_offset(&mut self, offset: UtcOffset, cx: &mut Context<'_, Self>) {
        if self.clock.preview_utc_offset(offset) {
            cx.notify();
        }
    }

    pub(crate) fn set_utc_offset(&mut self, offset: UtcOffset, cx: &mut Context<'_, Self>) {
        if self.clock.set_utc_offset(offset) {
            cx.notify();
        }
    }

    /// Saves a timezone left by a drag. Returns whether there was one.
    pub(crate) fn commit_utc_offset(&mut self) -> bool {
        self.clock.commit()
    }

    pub(crate) fn activate_decrease_utc_offset(
        &mut self,
        _: &ActivateEvent,
        cx: &mut Context<'_, Self>,
    ) {
        let offset = self.clock.utc_offset().step(-1);
        self.set_utc_offset(offset, cx);
    }

    pub(crate) fn activate_increase_utc_offset(
        &mut self,
        _: &ActivateEvent,
        cx: &mut Context<'_, Self>,
    ) {
        let offset = self.clock.utc_offset().step(1);
        self.set_utc_offset(offset, cx);
    }

    pub(crate) fn take_clock_preferences_request(&mut self) -> Option<ClockPreferences> {
        self.clock.take_save_request()
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
