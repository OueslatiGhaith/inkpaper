mod cover;
mod dictionary;
mod persistence;
mod platform;
mod reader;

use alloc::{string::String, vec::Vec};

use inkpaper_ui::{Entity, EntityAccessError, ImageSource, ResourceRuntimeApi, RuntimeApi};

use crate::{
    BookCover, BrowseListing, BrowseRequest, ClockPreferences, ClockSyncFailure,
    FileTransferRequest, FrontlightPreferences, InkPaperApp, ReaderPreferences, ReaderSession,
    ReadingHistory, ReadingHistoryRequest, WifiJoinFailure, WifiScanError,
    dictionaries::{DictionaryFolder, DictionaryRequest, find_dictionaries},
    fonts::{CardFont, FontFamily, find_font_families},
};

use dictionary::OpenDictionary;
pub use platform::{AppPlatform, PlatformEntry};

#[derive(Debug)]
pub enum AppServiceError {
    AppUnavailable(EntityAccessError),
}

impl From<EntityAccessError> for AppServiceError {
    fn from(error: EntityAccessError) -> Self {
        Self::AppUnavailable(error)
    }
}

pub struct AppService<P>
where
    P: AppPlatform,
{
    platform: P,

    reader_session: Option<ReaderSession<P::RandomAccessSource>>,

    /// the current book's cover, kept to register again after the reader
    /// clears its images
    cover: Option<BookCover>,
    /// the cover's registration, until the reader's images clear it
    cover_source: Option<ImageSource>,

    /// the font families on the card, found at startup
    font_families: Vec<FontFamily>,
    /// the card font read into memory and registered for the screen
    card_font: Option<CardFont>,
    /// a card family that couldn't be loaded, not tried again until another
    /// font is wanted
    failed_font: Option<u8>,

    /// the dictionaries on the card, found at startup
    dictionaries: Vec<DictionaryFolder>,
    /// the dictionary words were last looked up in, kept open for the next
    dictionary: Option<OpenDictionary<P::RandomAccessSource>>,
    /// a lookup put off until a frame shows its dictionary being indexed
    deferred_lookup: Option<DictionaryRequest>,

    history: ReadingHistory,
    preferences: ReaderPreferences,
    frontlight_preferences: FrontlightPreferences,
    clock_preferences: ClockPreferences,

    history_dirty: bool,
    preferences_dirty: bool,
    frontlight_preferences_dirty: bool,
    clock_preferences_dirty: bool,

    initialized: bool,
}

impl<P> AppService<P>
where
    P: AppPlatform,
{
    pub fn new(platform: P) -> Self {
        Self {
            platform,
            reader_session: None,
            cover: None,
            cover_source: None,

            font_families: Vec::new(),
            card_font: None,
            failed_font: None,

            dictionaries: Vec::new(),
            dictionary: None,
            deferred_lookup: None,

            history: ReadingHistory::default(),
            preferences: ReaderPreferences::default(),
            frontlight_preferences: FrontlightPreferences::default(),
            clock_preferences: ClockPreferences::default(),

            history_dirty: false,
            preferences_dirty: false,
            frontlight_preferences_dirty: false,
            clock_preferences_dirty: false,

            initialized: false,
        }
    }

    pub async fn service_pending<'resource, R>(
        &mut self,
        runtime: &mut R,
        app: Entity<InkPaperApp>,
    ) -> Result<(), AppServiceError>
    where
        R: RuntimeApi + ResourceRuntimeApi<'resource>,
    {
        self.ensure_initialized(runtime, app).await?;

        loop {
            // before the reader's requests, which lay out with the font
            let font = runtime.update(app, |app, _| app.wanted_reader_font())?;

            if self.service_reader_font(runtime, app, font).await? {
                continue;
            }

            let frontlight_request =
                runtime.update(app, |app, cx| app.take_frontlight_request(cx))?;

            if let Some(setting) = frontlight_request {
                let _ = self.platform.set_frontlight(setting).await;
                continue;
            }

            let frontlight_preferences_request =
                runtime.update(app, |app, cx| app.take_frontlight_preferences_request(cx))?;

            if let Some(request) = frontlight_preferences_request {
                self.service_frontlight_preferences_request(request).await;
                continue;
            }

            let clock_sync_request = runtime.update(app, |app, _| app.take_clock_sync_request())?;

            if let Some(plan) = clock_sync_request {
                if self.platform.start_clock_sync(&plan).await.is_err() {
                    runtime.update(app, |app, cx| {
                        app.apply_clock_sync_result(None, Err(ClockSyncFailure::Radio), cx);
                    })?;
                }

                continue;
            }

            let wifi_join_request = runtime.update(app, |app, _| app.take_wifi_join_request())?;

            if let Some(network) = wifi_join_request {
                if self.platform.start_wifi_join(&network).await.is_err() {
                    runtime.update(app, |app, cx| {
                        app.apply_wifi_join_result(Err(WifiJoinFailure::Radio), cx);
                    })?;
                }

                continue;
            }

            if runtime.update(app, |app, _| app.take_wifi_scan_request())? {
                if self.platform.start_wifi_scan().await.is_err() {
                    runtime.update(app, |app, cx| {
                        app.apply_wifi_scan_result(Err(WifiScanError), cx);
                    })?;
                }

                continue;
            }

            let wifi_networks_save =
                runtime.update(app, |app, _| app.take_wifi_networks_save_request())?;

            if let Some(networks) = wifi_networks_save {
                self.save_wifi_networks(&networks).await;
                continue;
            }

            let clock_preferences_request =
                runtime.update(app, |app, _| app.take_clock_preferences_request())?;

            if let Some(preferences) = clock_preferences_request {
                self.clock_preferences = preferences;
                self.clock_preferences_dirty = true;
                self.persist_clock_preferences().await;
                continue;
            }

            let preferences_request =
                runtime.update(app, |app, _| app.take_reader_preferences_request())?;

            if let Some(request) = preferences_request {
                self.service_reader_preferences_request(runtime, app, request)
                    .await?;

                continue;
            }

            if let Some(request) = self.deferred_lookup.take() {
                self.service_dictionary_request(runtime, app, request, false)
                    .await?;

                continue;
            }

            let dictionary_request = runtime.update(app, |app, _| app.take_dictionary_request())?;

            if let Some(request) = dictionary_request {
                self.service_dictionary_request(runtime, app, request, true)
                    .await?;

                // the frame showing the indexing goes first
                if self.deferred_lookup.is_some() {
                    return Ok(());
                }

                continue;
            }

            let cover_request = runtime.update(app, |app, _| app.take_cover_request())?;

            if let Some(path) = cover_request {
                self.service_cover_request(runtime, app, path).await?;
                continue;
            }

            let (browse_request, reader_request, history_request, file_transfer_request) = runtime
                .update(app, |app, _| {
                    (
                        app.take_browse_request(),
                        app.take_reader_request(),
                        app.take_reading_history_request(),
                        app.take_file_transfer_request(),
                    )
                })?;

            if browse_request.is_none()
                && reader_request.is_none()
                && history_request.is_none()
                && file_transfer_request.is_none()
            {
                return Ok(());
            }

            if let Some(request) = browse_request {
                self.service_browse_request(runtime, app, request).await?;
            }

            // Reader requests intentionally precede history snapshots.
            // A page turn may queue a progress update which should be reflected before
            // Home/Recent Books receives its snapshot.
            if let Some(request) = reader_request {
                self.service_reader_request(runtime, app, request).await?;
            }

            if let Some(request) = history_request {
                self.service_reading_history_request(runtime, app, request)?;
            }

            if let Some(request) = file_transfer_request {
                self.service_file_transfer_request(runtime, app, request)
                    .await?;

                // A successful USB Drive transition makes the filesystem unavailable.
                // Do not loop around and try to service another storage-backed request.
                return Ok(());
            }
        }
    }

    /// Whether [`AppService::service_pending`] put work off until the frame
    /// it left is shown, like a dictionary's first indexing. Once that frame
    /// is on screen, service again without waiting for input.
    pub const fn has_deferred_work(&self) -> bool {
        self.deferred_lookup.is_some()
    }

    pub async fn flush(&mut self) -> bool {
        let history_saved = self.persist_history().await;
        let preferences_saved = self.persist_preferences().await;
        let frontlight_preferences_saved = self.persist_frontlight_preferences().await;
        let clock_preferences_saved = self.persist_clock_preferences().await;

        history_saved
            && preferences_saved
            && frontlight_preferences_saved
            && clock_preferences_saved
    }

    async fn ensure_initialized<'resource, R>(
        &mut self,
        runtime: &mut R,
        app: Entity<InkPaperApp>,
    ) -> Result<(), AppServiceError>
    where
        R: RuntimeApi + ResourceRuntimeApi<'resource>,
    {
        if self.initialized {
            return Ok(());
        }

        // the saved font and dictionary are looked up among those found
        self.font_families = find_font_families(&mut self.platform).await;
        self.dictionaries = find_dictionaries(&mut self.platform).await;

        self.history = self.load_history().await;
        self.preferences = self.load_preferences().await;
        self.frontlight_preferences = self.load_frontlight_preferences().await;
        self.clock_preferences = self.load_clock_preferences().await;
        let wifi_networks = self.load_wifi_networks().await;

        let entries = self.history.entries().to_vec();
        let preferences = self.preferences;
        let font_names = self
            .font_families
            .iter()
            .map(|family| String::from(family.name()))
            .collect();
        let dictionary_names = self
            .dictionaries
            .iter()
            .map(|dictionary| String::from(dictionary.name()))
            .collect();
        let frontlight_preferences = self.frontlight_preferences;
        let clock_preferences = self.clock_preferences;

        runtime.update(app, move |app, cx| {
            app.apply_frontlight_preferences(frontlight_preferences, cx);
            app.apply_clock_preferences(clock_preferences, cx);
            app.apply_wifi_networks(wifi_networks, cx);
            app.apply_reader_preferences(preferences, cx);
            app.apply_reading_history(entries, cx);
            app.apply_font_families(font_names, cx);
            app.apply_dictionaries(dictionary_names, cx);
        })?;

        self.initialized = true;

        Ok(())
    }

    async fn service_browse_request<R>(
        &mut self,
        runtime: &mut R,
        app: Entity<InkPaperApp>,
        request: BrowseRequest,
    ) -> Result<(), AppServiceError>
    where
        R: RuntimeApi,
    {
        match request {
            BrowseRequest::ListDirectory(path) => match self.platform.list_directory(&path).await {
                Ok(entries) => {
                    let entries = entries
                        .into_iter()
                        .map(PlatformEntry::into_browse_entry)
                        .collect();

                    let listing = BrowseListing::new(path, entries);

                    runtime.update(app, move |app, cx| {
                        app.apply_browse_listing(listing, cx);
                    })?;
                }

                Err(_) => {
                    runtime.update(app, |app, cx| {
                        app.apply_browse_error(cx);
                    })?;
                }
            },
        }

        Ok(())
    }

    fn service_reading_history_request<R>(
        &mut self,
        runtime: &mut R,
        app: Entity<InkPaperApp>,
        request: ReadingHistoryRequest,
    ) -> Result<(), AppServiceError>
    where
        R: RuntimeApi,
    {
        match request {
            ReadingHistoryRequest::Load => {
                let entries = self.history.entries().to_vec();

                runtime.update(app, move |app, cx| {
                    app.apply_reading_history(entries, cx);
                })?;
            }
        }

        Ok(())
    }

    async fn service_file_transfer_request<R>(
        &mut self,
        runtime: &mut R,
        app: Entity<InkPaperApp>,
        request: FileTransferRequest,
    ) -> Result<(), AppServiceError>
    where
        R: RuntimeApi,
    {
        match request {
            FileTransferRequest::EnterUsbDrive => {
                // Persist everything while FAT is still mounted.
                //
                // Refuse the handoff if this fails. Giving the host raw access after
                // failing to save application state would make it impossible to
                // recover that state until the next boot.
                if !self.flush().await {
                    runtime.update(app, |app, cx| {
                        app.apply_usb_drive_result(false, cx);
                    })?;

                    return Ok(());
                }

                // No app-side reader object may survive the storage ownership
                // transition. The storage service independently drops its FAT
                // OpenRandomAccessFile when it receives EnterUsbDrive.
                self.reader_session = None;
                self.dictionary = None;
                self.deferred_lookup = None;

                let ready = self.platform.enter_usb_drive().await.is_ok();

                runtime.update(app, move |app, cx| {
                    app.apply_usb_drive_result(ready, cx);
                })?;
            }
        }

        Ok(())
    }
}
