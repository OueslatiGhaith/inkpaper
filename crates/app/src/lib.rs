#![no_std]

extern crate alloc;

mod app;
mod browser;
mod clock;
mod components;
mod control_center;
mod file_transfer;
mod frontlight;
mod gesture;
mod input;
mod keyboard;
mod reader;
mod reader_page;
mod reading_history;
mod screens;
mod service;
mod system;
mod typography;
mod wifi;

pub use app::InkPaperApp;
pub use file_transfer::UsbDriveConnection;
pub use frontlight::FrontlightSetting;
pub use gesture::{LONG_PRESS_MS, TouchGesture};
pub use service::{AppPlatform, AppService, AppServiceError, PlatformEntry};
pub use system::{BatteryStatus, ClockStatus};
pub use wifi::{
    DeviceKey, WifiCredentials, WifiJoinFailure, WifiJoinPlan, WifiNetwork, WifiScanError,
};

// internal application domain/effect types.
//
// keep these available through `crate::...` so the existing internal modules don't need
// artificial public APIs, while preventing platform wrappers from depending on them.
pub(crate) use browser::{BrowseEntry, BrowseEntryKind, BrowseListing, BrowseRequest};
pub use clock::ClockSyncFailure;
pub(crate) use clock::{ClockPreferences, ClockState, ClockSyncStatus, UtcOffset};
pub(crate) use file_transfer::{FileTransferRequest, FileTransferState, FileTransferStatus};
pub(crate) use frontlight::{
    FrontlightPreferences, FrontlightPreferencesError, FrontlightPreferencesRequest,
    FrontlightState,
};
pub use input::{AppInputError, AppInputEvent, dispatch_input};
pub(crate) use reader::{
    ReaderChapter, ReaderChapterDirection, ReaderDocument, ReaderLoadError, ReaderPreferences,
    ReaderPreferencesError, ReaderPreferencesRequest, ReaderRequest, ReaderSession,
    load_adjacent_reader_chapter, load_reader_document,
};
pub(crate) use reading_history::{
    BookProgress, MAX_READING_HISTORY_ENTRIES, ReadingHistory, ReadingHistoryEntry,
    ReadingHistoryError, ReadingHistoryRequest,
};
pub(crate) use wifi::{SavedNetworks, WifiScanStatus};
