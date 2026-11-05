use alloc::string::String;

use inkpaper_ui::prelude::*;

use super::InkPaperApp;
use crate::{
    BrowseRequest, ClockPreferences, FileTransferRequest, FrontlightPreferencesRequest,
    FrontlightSetting, ReaderPreferencesRequest, ReaderRequest, ReadingHistoryRequest,
    SavedNetworks, WifiCredentials, WifiJoinPlan, dictionaries::DictionaryRequest,
};

/// work the app asks the service for
#[derive(Debug)]
pub(crate) enum AppRequest {
    Frontlight(FrontlightSetting),
    FrontlightPreferences(FrontlightPreferencesRequest),
    /// sync the clock over the network this plan joins
    ClockSync(WifiJoinPlan),
    WifiJoin(WifiCredentials),
    WifiScan,
    SaveWifiNetworks(SavedNetworks),
    SaveClockPreferences(ClockPreferences),
    ReaderPreferences(ReaderPreferencesRequest),
    Dictionary(DictionaryRequest),
    /// load the cover of the book at this path
    Cover(String),
    Browse(BrowseRequest),
    Reader(ReaderRequest),
    ReadingHistory(ReadingHistoryRequest),
    FileTransfer(FileTransferRequest),
}

impl InkPaperApp {
    /// the next request for the service. Each kind waits in its own slot, where a
    /// newer request replaces an older one, and they come out in this order: the
    /// quick device and settings work first, then lookups and covers, then storage.
    /// Reader requests come before history, so a page turn's progress is saved before
    /// Home and Recent Books take their snapshot
    pub(crate) fn take_request(&mut self, cx: &mut Context<'_, Self>) -> Option<AppRequest> {
        self.frontlight
            .update(cx, |frontlight, _| {
                frontlight
                    .take_request()
                    .map(AppRequest::Frontlight)
                    .or_else(|| {
                        frontlight
                            .take_preferences_request()
                            .map(AppRequest::FrontlightPreferences)
                    })
            })
            .or_else(|| self.take_clock_sync_request().map(AppRequest::ClockSync))
            .or_else(|| self.wifi.take_join_request().map(AppRequest::WifiJoin))
            .or_else(|| {
                self.wifi
                    .take_scan_request()
                    .then_some(AppRequest::WifiScan)
            })
            .or_else(|| {
                self.wifi
                    .take_save_request()
                    .map(AppRequest::SaveWifiNetworks)
            })
            .or_else(|| {
                self.clock
                    .take_save_request()
                    .map(AppRequest::SaveClockPreferences)
            })
            .or_else(|| {
                self.reader
                    .take_preferences_request()
                    .map(AppRequest::ReaderPreferences)
            })
            .or_else(|| self.lookup.take_request().map(AppRequest::Dictionary))
            .or_else(|| self.cover.take_request().map(AppRequest::Cover))
            .or_else(|| self.browser.take_request().map(AppRequest::Browse))
            .or_else(|| self.reader.take_request().map(AppRequest::Reader))
            .or_else(|| {
                self.reading_history
                    .take_request()
                    .map(AppRequest::ReadingHistory)
            })
            .or_else(|| {
                self.file_transfer
                    .take_request()
                    .map(AppRequest::FileTransfer)
            })
    }

    /// a sync the clock wants, when there's a saved network to sync over
    fn take_clock_sync_request(&mut self) -> Option<WifiJoinPlan> {
        if !self.clock.take_sync_request() {
            return None;
        }

        self.wifi.saved().join_plan()
    }
}
