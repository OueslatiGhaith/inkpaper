mod cover;
mod dictionary;
mod persistence;
mod platform;
mod reader;

use alloc::{string::String, vec::Vec};

use inkpaper_ui::{
    Entity, EntityAccessError, ImageId, ImageSource, ResourceRuntimeApi, RuntimeApi,
};

use crate::{
    BookCover, BrowseListing, BrowseRequest, ClockPreferences, ClockSyncFailure,
    FileTransferRequest, FrontlightPreferences, InkPaperApp, ReaderPreferences, ReaderSession,
    ReadingHistory, ReadingHistoryRequest, SavedNetworks, WifiJoinFailure, WifiScanError,
    app::AppRequest,
    dictionaries::{DictionaryFolder, find_dictionaries},
    fonts::{CardFont, FontFamily, find_font_families},
    resume::SleptBook,
};

use dictionary::OpenDictionary;
use persistence::Persisted;
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
    /// images currently registered in the UI registry for the open reader chapter
    reader_image_ids: Vec<ImageId>,

    /// the current book's cover thumbnail for Home
    cover: Option<BookCover>,
    /// the cover's registered slot in the UI image registry
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
    /// whether long work was put off until the screen shows what asked for it
    deferred: bool,

    history: Persisted<ReadingHistory>,
    /// the book being read when the device went to sleep, to reopen at boot
    resume: Persisted<Option<SleptBook>>,
    preferences: Persisted<ReaderPreferences>,
    frontlight_preferences: Persisted<FrontlightPreferences>,
    clock_preferences: Persisted<ClockPreferences>,
    wifi_networks: Persisted<SavedNetworks>,

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
            reader_image_ids: Vec::new(),
            cover: None,
            cover_source: None,

            font_families: Vec::new(),
            card_font: None,
            failed_font: None,

            dictionaries: Vec::new(),
            dictionary: None,
            deferred: false,

            history: Persisted::default(),
            resume: Persisted::default(),
            preferences: Persisted::default(),
            frontlight_preferences: Persisted::default(),
            clock_preferences: Persisted::default(),
            wifi_networks: Persisted::default(),

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
        self.deferred = false;

        loop {
            // before the reader's requests, which lay out with the font
            let font = runtime.update(app, |app, _| app.wanted_reader_font())?;

            if self.service_reader_font(runtime, app, font).await? {
                continue;
            }

            let Some(request) = runtime.update(app, |app, cx| app.take_request(cx))? else {
                return Ok(());
            };

            match request {
                AppRequest::Frontlight(setting) => {
                    let _ = self.platform.set_frontlight(setting).await;
                }
                AppRequest::FrontlightPreferences(request) => {
                    self.service_frontlight_preferences_request(request).await;
                }
                AppRequest::ClockSync(plan) => {
                    if self.platform.start_clock_sync(&plan).await.is_err() {
                        runtime.update(app, |app, cx| {
                            app.apply_clock_sync_result(None, Err(ClockSyncFailure::Radio), cx);
                        })?;
                    }
                }
                AppRequest::WifiJoin(token, network) => {
                    if self.platform.start_wifi_join(token, &network).await.is_err() {
                        runtime.update(app, |app, cx| {
                            app.apply_wifi_join_result(token, Err(WifiJoinFailure::Radio), cx);
                        })?;
                    }
                }
                AppRequest::WifiScan => {
                    if self.platform.start_wifi_scan().await.is_err() {
                        runtime.update(app, |app, cx| {
                            app.apply_wifi_scan_result(Err(WifiScanError), cx);
                        })?;
                    }
                }
                AppRequest::SaveWifiNetworks(networks) => {
                    self.wifi_networks.set(networks);
                    self.persist_wifi_networks().await;
                }
                AppRequest::SaveClockPreferences(preferences) => {
                    self.clock_preferences.set(preferences);
                    self.persist_clock_preferences().await;
                }
                AppRequest::SaveReaderPreferences(preferences) => {
                    self.preferences.set(preferences);
                    self.persist_preferences().await;
                }
                AppRequest::Dictionary(request) => {
                    self.service_dictionary_request(runtime, app, request)
                        .await?;

                    // it waits for the frame showing the indexing
                    if self.deferred {
                        return Ok(());
                    }
                }
                AppRequest::Cover(path) => {
                    self.service_cover_request(runtime, app, path).await?;
                }
                AppRequest::Browse(request) => {
                    self.service_browse_request(runtime, app, request).await?;
                }
                // the screen shows what asked for it first
                AppRequest::Reader(request)
                    if request.takes_long() && runtime.has_pending_frame() =>
                {
                    self.deferred = true;
                    runtime.update(app, move |app, _| app.restore_reader_request(request))?;

                    return Ok(());
                }
                AppRequest::Reader(request) => {
                    self.service_reader_request(runtime, app, request).await?;
                }
                AppRequest::ReadingHistory(request) => {
                    self.service_reading_history_request(runtime, app, request)?;
                }
                AppRequest::FileTransfer(request) => {
                    self.service_file_transfer_request(runtime, app, request)
                        .await?;

                    // A successful USB Drive transition makes the filesystem unavailable.
                    // Do not loop around and try to service another storage-backed request.
                    return Ok(());
                }
            }
        }
    }

    /// Whether [`AppService::service_pending`] put long work off until the
    /// frame it left is shown, like opening a book or a dictionary's first
    /// indexing. Once that frame is on screen, service again without waiting
    /// for input.
    pub const fn has_deferred_work(&self) -> bool {
        self.deferred
    }

    /// Shows the sleep screen, and remembers the book being read for the
    /// next boot to reopen, like crosspoint. [`AppService::flush`] saves it
    /// with the rest.
    pub fn prepare_for_sleep(
        &mut self,
        runtime: &mut impl RuntimeApi,
        app: Entity<InkPaperApp>,
    ) -> Result<(), AppServiceError> {
        let resume = runtime.update(app, |app, cx| {
            app.prepare_for_sleep(cx);
            app.book_to_resume(cx)
        })?;
        self.resume.set(resume);

        Ok(())
    }

    pub async fn flush(&mut self) -> bool {
        let history_saved = self.persist_history().await;
        let resume_saved = self.persist_resume().await;
        let preferences_saved = self.persist_preferences().await;
        let frontlight_preferences_saved = self.persist_frontlight_preferences().await;
        let clock_preferences_saved = self.persist_clock_preferences().await;
        let wifi_networks_saved = self.persist_wifi_networks().await;

        history_saved
            && resume_saved
            && preferences_saved
            && frontlight_preferences_saved
            && clock_preferences_saved
            && wifi_networks_saved
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

        let history = self.load_history().await;
        self.history.load(history);
        let preferences = self.load_preferences().await;
        self.preferences.load(preferences);
        let frontlight_preferences = self.load_frontlight_preferences().await;
        self.frontlight_preferences.load(frontlight_preferences);
        let clock_preferences = self.load_clock_preferences().await;
        self.clock_preferences.load(clock_preferences);
        let wifi_networks = self.load_wifi_networks().await;
        self.wifi_networks.load(wifi_networks.clone());
        let resume = self.take_resume().await;

        let entries = self.history.get().entries().to_vec();
        let preferences = *self.preferences.get();
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
        let frontlight_preferences = *self.frontlight_preferences.get();
        let clock_preferences = *self.clock_preferences.get();

        runtime.update(app, move |app, cx| {
            app.apply_frontlight_preferences(frontlight_preferences, cx);
            app.apply_clock_preferences(clock_preferences, cx);
            app.apply_wifi_networks(wifi_networks, cx);
            app.apply_reader_preferences(preferences, cx);
            app.apply_reading_history(entries, cx);
            app.apply_font_families(font_names, cx);
            app.apply_dictionaries(dictionary_names, cx);

            // after the preferences and history it opens with
            if let Some(book) = resume {
                app.resume_book(book, cx);
            }
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
            BrowseRequest::ListDirectory { id, path } => {
                match self.platform.list_directory(&path).await {
                    Ok(entries) => {
                        let entries = entries
                            .into_iter()
                            .map(PlatformEntry::into_browse_entry)
                            .collect();

                        let listing = BrowseListing::new(path, entries);

                        runtime.update(app, move |app, cx| {
                            app.apply_browse_listing(id, listing, cx);
                        })?;
                    }

                    Err(_) => {
                        runtime.update(app, move |app, cx| {
                            app.apply_browse_error(id, cx);
                        })?;
                    }
                }
            }
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
                let entries = self.history.get().entries().to_vec();

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
                self.deferred = false;

                let ready = self.platform.enter_usb_drive().await.is_ok();

                runtime.update(app, move |app, cx| {
                    app.apply_usb_drive_result(ready, cx);
                })?;
            }
        }

        Ok(())
    }
}
