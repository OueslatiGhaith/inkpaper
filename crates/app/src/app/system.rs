use inkpaper_ui::prelude::*;

use super::{InkPaperApp, Screen};
use crate::{
    BatteryStatus, ClockPreferences, ClockStatus, ClockSyncFailure, FrontlightPreferences,
    FrontlightPreferencesRequest, FrontlightSetting, UtcOffset, WifiJoinPlan,
};

impl InkPaperApp {
    /// Renders again what shows the battery: the entities and screens that read it, and
    /// the reader's icon when it changes. Nothing else renders, so reading a book
    /// doesn't wake the display for a new percentage.
    pub fn apply_battery_status(&mut self, battery: BatteryStatus, cx: &mut Context<'_, Self>) {
        self.battery
            .update(cx, |state, cx| {
                if state.set(battery) {
                    cx.notify();
                }
            })
            .ok();
        self.reader_battery_icon
            .update(cx, |icon, cx| icon.set_battery(battery, cx))
            .ok();
    }

    /// Renders again only what read the time, so the RTC's minute ticks don't wake the
    /// display on screens without a clock.
    pub fn apply_clock_status(&mut self, clock: Option<ClockStatus>, cx: &mut Context<'_, Self>) {
        self.rtc
            .update(cx, |state, cx| {
                if state.set(clock) {
                    cx.notify();
                }
            })
            .ok();
    }

    /// The RTC's UTC time shifted to the chosen timezone. A render that calls this renders
    /// again when the time changes.
    pub(crate) fn local_clock<T: 'static>(&self, cx: &Context<'_, T>) -> Option<ClockStatus> {
        let minutes = self.clock.utc_offset().minutes();

        self.rtc
            .read(cx, |rtc| rtc.get())
            .ok()
            .flatten()
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

    pub(crate) fn activate_sync_clock(&mut self, _: &ActivateEvent, cx: &mut Context<'_, Self>) {
        let changed = if self.wifi.saved().join_plan().is_some() {
            self.clock.request_sync()
        } else {
            self.clock.finish_sync(Err(ClockSyncFailure::NoNetwork))
        };

        if changed {
            cx.notify();
        }
    }

    /// The networks a requested sync may join.
    pub(crate) fn take_clock_sync_request(&mut self) -> Option<WifiJoinPlan> {
        if !self.clock.take_sync_request() {
            return None;
        }

        self.wifi.saved().join_plan()
    }

    /// Reports how a sync started by the app ended, and which network it
    /// joined, if any. That network becomes the connected one, like
    /// crosspoint's last connected network. On success the platform also
    /// delivers the new time through [`Self::apply_clock_status`].
    pub fn apply_clock_sync_result(
        &mut self,
        joined: Option<&str>,
        result: Result<(), ClockSyncFailure>,
        cx: &mut Context<'_, Self>,
    ) {
        let connected = joined
            .and_then(|ssid| self.wifi.saved().find(ssid))
            .map(|network| network.credentials().clone())
            .is_some_and(|credentials| self.wifi.connect(credentials));

        let finished = self.clock.finish_sync(result) && self.screen() == Screen::ClockSettings;

        if connected || finished {
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
