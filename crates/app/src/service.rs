use alloc::{string::String, vec::Vec};

use alloc::boxed::Box;

use inkpaper_epub::{Epub, EpubSource};
use inkpaper_ui::{Entity, EntityAccessError, ImageSource, ResourceRuntimeApi, RuntimeApi};

use crate::{
    BookCover, BrowseEntry, BrowseListing, BrowseRequest, ClockPreferences, ClockSyncFailure,
    DeviceKey, FileTransferRequest, FrontlightPreferences, FrontlightPreferencesRequest,
    FrontlightSetting, InkPaperApp, ReaderPreferences, ReaderPreferencesRequest, ReaderRequest,
    ReaderSession, ReadingHistory, ReadingHistoryRequest, SavedNetworks, WifiCredentials,
    WifiJoinFailure, WifiJoinPlan, WifiScanError,
    cover::read_cover_image,
    fonts::{CardFont, FontFamily, card_for, find_font_families, load_card_font},
    reader::{GrayImage, ReaderFont, TextSettings},
    typography::register_card_font,
};

const READING_HISTORY_STATE: &str = "reading-history.dat";
const READER_PREFERENCES_STATE: &str = "reader-preferences.dat";
const FRONTLIGHT_PREFERENCES_STATE: &str = "frontlight-preferences.dat";
const MAX_FRONTLIGHT_PREFERENCES_BYTES: usize = 64;
const CLOCK_PREFERENCES_STATE: &str = "clock-preferences.dat";
const MAX_CLOCK_PREFERENCES_BYTES: usize = 64;
const WIFI_NETWORKS_STATE: &str = "wifi-networks.dat";
const MAX_WIFI_NETWORKS_BYTES: usize = 1024;
/// the current book's cover thumbnail
const COVER_STATE: &str = "cover.dat";
const MAX_COVER_BYTES: usize = 48 * 1024;

const MAX_READING_HISTORY_BYTES: usize = 64 * 1024;
const MAX_READER_PREFERENCES_BYTES: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlatformEntryKind {
    Directory,
    File,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformEntry {
    name: String,
    kind: PlatformEntryKind,
}

impl PlatformEntry {
    pub fn directory(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            kind: PlatformEntryKind::Directory,
        }
    }

    pub fn file(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            kind: PlatformEntryKind::File,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub const fn is_directory(&self) -> bool {
        matches!(self.kind, PlatformEntryKind::Directory)
    }

    fn into_browse_entry(self) -> BrowseEntry {
        match self.kind {
            PlatformEntryKind::Directory => BrowseEntry::directory(self.name),
            PlatformEntryKind::File => BrowseEntry::file(self.name),
        }
    }
}

#[allow(async_fn_in_trait)]
pub trait AppPlatform {
    type Error;
    type RandomAccessSource: EpubSource;

    async fn list_directory(&mut self, path: &str) -> Result<Vec<PlatformEntry>, Self::Error>;

    async fn open_random_access(
        &mut self,
        path: &str,
    ) -> Result<Self::RandomAccessSource, Self::Error>;

    /// Reads the whole file at `path`, refusing one over `max_bytes`.
    async fn read_file(&mut self, path: &str, max_bytes: usize) -> Result<Vec<u8>, Self::Error>;

    async fn set_frontlight(&mut self, setting: FrontlightSetting) -> Result<(), Self::Error>;

    async fn load_state(
        &mut self,
        name: &str,
        max_bytes: usize,
    ) -> Result<Option<Vec<u8>>, Self::Error>;

    async fn save_state(&mut self, name: &str, bytes: &[u8]) -> Result<(), Self::Error>;

    async fn enter_usb_drive(&mut self) -> Result<(), Self::Error>;

    /// Bytes unique to this device, used to obfuscate saved WiFi passwords.
    fn device_key(&self) -> DeviceKey;

    /// Starts setting the clock over a network from `plan` and returns without
    /// waiting. The connected network comes first; when it can't be joined,
    /// the platform scans and tries the fallbacks in range. It reports the
    /// outcome through [`InkPaperApp::apply_clock_sync_result`].
    async fn start_clock_sync(&mut self, plan: &WifiJoinPlan) -> Result<(), Self::Error>;

    /// Starts joining `network` to check its password, then leaves it, and
    /// returns without waiting. The platform reports the outcome through
    /// [`InkPaperApp::apply_wifi_join_result`].
    async fn start_wifi_join(&mut self, network: &WifiCredentials) -> Result<(), Self::Error>;

    /// Starts a WiFi scan and returns without waiting. The platform reports the
    /// networks through [`InkPaperApp::apply_wifi_scan_result`].
    async fn start_wifi_scan(&mut self) -> Result<(), Self::Error>;
}

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

        // the saved font is looked up among the families
        self.font_families = find_font_families(&mut self.platform).await;

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
        let frontlight_preferences = self.frontlight_preferences;
        let clock_preferences = self.clock_preferences;

        runtime.update(app, move |app, cx| {
            app.apply_frontlight_preferences(frontlight_preferences, cx);
            app.apply_clock_preferences(clock_preferences, cx);
            app.apply_wifi_networks(wifi_networks, cx);
            app.apply_reader_preferences(preferences, cx);
            app.apply_reading_history(entries, cx);
            app.apply_font_families(font_names, cx);
        })?;

        self.initialized = true;

        Ok(())
    }

    async fn load_history(&mut self) -> ReadingHistory {
        let bytes = match self
            .platform
            .load_state(READING_HISTORY_STATE, MAX_READING_HISTORY_BYTES)
            .await
        {
            Ok(Some(bytes)) => bytes,
            Ok(None) | Err(_) => {
                return ReadingHistory::default();
            }
        };

        ReadingHistory::decode(&bytes).unwrap_or_default()
    }

    async fn load_preferences(&mut self) -> ReaderPreferences {
        let bytes = match self
            .platform
            .load_state(READER_PREFERENCES_STATE, MAX_READER_PREFERENCES_BYTES)
            .await
        {
            Ok(Some(bytes)) => bytes,
            Ok(None) | Err(_) => {
                return ReaderPreferences::default();
            }
        };

        let families = &self.font_families;

        ReaderPreferences::decode(&bytes, |name| {
            let index = families.iter().position(|family| family.name() == name)?;

            u8::try_from(index).ok().map(ReaderFont::Card)
        })
        .unwrap_or_default()
    }

    async fn load_frontlight_preferences(&mut self) -> FrontlightPreferences {
        let bytes = match self
            .platform
            .load_state(
                FRONTLIGHT_PREFERENCES_STATE,
                MAX_FRONTLIGHT_PREFERENCES_BYTES,
            )
            .await
        {
            Ok(Some(bytes)) => bytes,
            Ok(None) | Err(_) => {
                return FrontlightPreferences::default();
            }
        };

        FrontlightPreferences::decode(&bytes).unwrap_or_default()
    }

    async fn load_clock_preferences(&mut self) -> ClockPreferences {
        let bytes = match self
            .platform
            .load_state(CLOCK_PREFERENCES_STATE, MAX_CLOCK_PREFERENCES_BYTES)
            .await
        {
            Ok(Some(bytes)) => bytes,
            Ok(None) | Err(_) => {
                return ClockPreferences::default();
            }
        };

        ClockPreferences::decode(&bytes).unwrap_or_default()
    }

    async fn load_wifi_networks(&mut self) -> SavedNetworks {
        let bytes = match self
            .platform
            .load_state(WIFI_NETWORKS_STATE, MAX_WIFI_NETWORKS_BYTES)
            .await
        {
            Ok(Some(bytes)) => bytes,
            Ok(None) | Err(_) => {
                return SavedNetworks::default();
            }
        };

        SavedNetworks::decode(&bytes, &self.platform.device_key()).unwrap_or_default()
    }

    /// Networks are saved as soon as one is chosen; they change rarely.
    async fn save_wifi_networks(&mut self, networks: &SavedNetworks) -> bool {
        let Ok(bytes) = networks.encode(&self.platform.device_key()) else {
            return false;
        };

        self.platform
            .save_state(WIFI_NETWORKS_STATE, &bytes)
            .await
            .is_ok()
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

    async fn service_reader_request<'resource, R>(
        &mut self,
        runtime: &mut R,
        app: Entity<InkPaperApp>,
        request: ReaderRequest,
    ) -> Result<(), AppServiceError>
    where
        R: RuntimeApi + ResourceRuntimeApi<'resource>,
    {
        match request {
            ReaderRequest::OpenEpub { path, text } => {
                self.open_document(runtime, app, path, text).await?;
            }

            ReaderRequest::LoadAdjacentChapter {
                path,
                from,
                direction,
            } => {
                let chapter = match self.reader_session.as_mut() {
                    Some(session) if session.path() == path => session
                        .load_adjacent_chapter(from, direction)
                        .await
                        .ok()
                        .flatten(),

                    _ => None,
                };

                match chapter {
                    Some(mut chapter) => {
                        chapter.register_images(runtime);
                        self.cover_source = None;

                        runtime.update(app, move |app, cx| {
                            app.apply_reader_chapter(path, from, direction, chapter, cx)
                        })?;
                    }

                    None => {
                        runtime.update(app, move |app, _| {
                            app.finish_reader_chapter_request(path, from, direction)
                        })?;
                    }
                }
            }

            ReaderRequest::RepaginateChapter { path, spine, text } => {
                let chapter = match self.reader_session.as_mut() {
                    Some(session) if session.path() == path => {
                        let card = card_for(text.font(), self.card_font.as_ref());

                        session
                            .repaginate_chapter(spine, text, card)
                            .await
                            .ok()
                            .flatten()
                    }

                    _ => None,
                };

                match chapter {
                    Some(mut chapter) => {
                        chapter.register_images(runtime);
                        self.cover_source = None;

                        runtime.update(app, move |app, cx| {
                            app.apply_reader_repagination(path, spine, text, chapter, cx);
                        })?;
                    }

                    None => {
                        runtime.update(app, move |app, _| {
                            app.finish_reader_repagination_request(path, spine, text);
                        })?;
                    }
                }
            }

            ReaderRequest::JumpTo { path, target } => {
                let landing = match self.reader_session.as_mut() {
                    Some(session) if session.path() == path => {
                        session.load_jump_target(&target).await.ok().flatten()
                    }

                    _ => None,
                };

                match landing {
                    Some((mut chapter, page_index)) => {
                        chapter.register_images(runtime);
                        self.cover_source = None;

                        runtime.update(app, move |app, cx| {
                            app.apply_reader_jump(path, target, chapter, page_index, cx);
                        })?;
                    }

                    None => {
                        runtime.update(app, move |app, _| {
                            app.finish_reader_jump_request(path, target);
                        })?;
                    }
                }
            }

            ReaderRequest::LoadTableOfContents { path } => {
                let entries = match self.reader_session.as_mut() {
                    Some(session) if session.path() == path => {
                        session.load_table_of_contents().await.ok()
                    }

                    _ => None,
                };

                runtime.update(app, move |app, cx| match entries {
                    Some(entries) => app.apply_reader_table_of_contents(path, entries, cx),
                    None => app.apply_reader_table_of_contents_error(path, cx),
                })?;
            }

            ReaderRequest::UpdateProgress(progress) => {
                self.history.record(progress);
                self.history_dirty = true;
            }
        }

        Ok(())
    }

    async fn open_document<'resource, R>(
        &mut self,
        runtime: &mut R,
        app: Entity<InkPaperApp>,
        path: String,
        text: TextSettings,
    ) -> Result<(), AppServiceError>
    where
        R: RuntimeApi + ResourceRuntimeApi<'resource>,
    {
        let reuse = self
            .reader_session
            .as_ref()
            .is_some_and(|session| session.path() == path);

        if !reuse {
            self.reader_session = None;

            let source = match self.platform.open_random_access(&path).await {
                Ok(source) => source,

                Err(_) => {
                    runtime.update(app, move |app, cx| {
                        app.apply_reader_error(path, cx);
                    })?;

                    return Ok(());
                }
            };

            let session = match ReaderSession::open(path.clone(), source).await {
                Ok(session) => session,

                Err(_) => {
                    runtime.update(app, move |app, cx| {
                        app.apply_reader_error(path, cx);
                    })?;

                    return Ok(());
                }
            };

            self.reader_session = Some(session);
        }

        let document = {
            let Some(session) = self.reader_session.as_mut() else {
                return Ok(());
            };

            let card = card_for(text.font(), self.card_font.as_ref());

            if session.set_text_settings(text, card).is_err() {
                return Ok(());
            }

            let resume = self.history.resume_position(&path, session.identifier());

            session.load_document_at(resume).await.ok()
        };

        match document {
            Some(mut document) => {
                document.register_images(runtime);
                self.cover_source = None;

                runtime.update(app, move |app, cx| {
                    app.apply_reader_document(document, cx);
                })?;
            }

            None => {
                runtime.update(app, move |app, cx| {
                    app.apply_reader_error(path, cx);
                })?;
            }
        }

        Ok(())
    }

    /// Shows the cover of the book at `path`: the one already registered, the
    /// cached thumbnail, or one made from the book and cached.
    async fn service_cover_request<'resource, R>(
        &mut self,
        runtime: &mut R,
        app: Entity<InkPaperApp>,
        path: String,
    ) -> Result<(), AppServiceError>
    where
        R: RuntimeApi + ResourceRuntimeApi<'resource>,
    {
        if self.cover.as_ref().is_none_or(|cover| cover.path() != path) {
            self.cover_source = None;
            self.cover = self.load_cover(&path).await;
        }

        let image = self.cover.as_ref().and_then(BookCover::image);

        if self.cover_source.is_none()
            && let Some(image) = image
        {
            self.cover_source = runtime.register_owned_image(Box::new(image.clone())).ok();
        }

        let source = self.cover_source;

        runtime.update(app, move |app, cx| app.apply_cover(path, source, cx))?;

        Ok(())
    }

    /// The cover of the book at `path`, from the cache or made from the book.
    /// `None` when the book couldn't be read, so it is tried again next time.
    async fn load_cover(&mut self, path: &str) -> Option<BookCover> {
        if let Ok(Some(bytes)) = self.platform.load_state(COVER_STATE, MAX_COVER_BYTES).await
            && let Ok(cover) = BookCover::decode(&bytes)
            && cover.path() == path
        {
            return Some(cover);
        }

        let image = self.read_cover_image(path).await?;
        let cover = BookCover::new(String::from(path), image.as_ref());

        if let Ok(bytes) = cover.encode() {
            let _ = self.platform.save_state(COVER_STATE, &bytes).await;
        }

        Some(cover)
    }

    async fn read_cover_image(&mut self, path: &str) -> Option<Option<GrayImage>> {
        // the platform may keep only one file open, so read through the
        // reader's book when it is this one
        if let Some(session) = self.reader_session.as_mut()
            && session.path() == path
        {
            return session.cover_image().await.ok();
        }

        // opening another file closes the reader's, so its session goes too
        self.reader_session = None;

        let source = self.platform.open_random_access(path).await.ok()?;
        let mut epub = Epub::open(source).await.ok()?;

        read_cover_image(&mut epub).await.ok()
    }

    /// Loads the card font the app wants, or frees the loaded one when it
    /// wants the built-in font, and registers it for the screen. Returns
    /// whether the fonts changed.
    async fn service_reader_font<'resource, R>(
        &mut self,
        runtime: &mut R,
        app: Entity<InkPaperApp>,
        font: ReaderFont,
    ) -> Result<bool, AppServiceError>
    where
        R: RuntimeApi + ResourceRuntimeApi<'resource>,
    {
        if self
            .failed_font
            .is_some_and(|failed| font != ReaderFont::Card(failed))
        {
            self.failed_font = None;
        }

        let loaded = self.card_font.as_ref().map(CardFont::family);

        let index = match font {
            ReaderFont::BuiltIn if loaded.is_none() => return Ok(false),
            ReaderFont::BuiltIn => None,

            ReaderFont::Card(index) if loaded == Some(index) => return Ok(false),
            ReaderFont::Card(index) if self.failed_font == Some(index) => return Ok(false),
            ReaderFont::Card(index) => Some(index),
        };

        // the old faces go before the new ones are read, so two families
        // aren't in memory at once
        if self.card_font.take().is_some() {
            let _ = register_card_font(runtime, None);
            runtime.update(app, |app, cx| app.apply_card_font(None, cx))?;
        }

        let Some(index) = index else {
            return Ok(true);
        };

        let card = match load_card_font(&mut self.platform, &self.font_families, index).await {
            Ok(card) if register_card_font(runtime, Some(&card)).is_ok() => card,

            _ => {
                let _ = register_card_font(runtime, None);
                self.failed_font = Some(index);

                return Ok(true);
            }
        };

        self.card_font = Some(card.clone());
        runtime.update(app, move |app, cx| app.apply_card_font(Some(card), cx))?;

        Ok(true)
    }

    async fn service_reader_preferences_request<R>(
        &mut self,
        runtime: &mut R,
        app: Entity<InkPaperApp>,
        request: ReaderPreferencesRequest,
    ) -> Result<(), AppServiceError>
    where
        R: RuntimeApi,
    {
        match request {
            ReaderPreferencesRequest::Load => {
                let preferences = self.preferences;

                runtime.update(app, move |app, cx| {
                    app.apply_reader_preferences(preferences, cx);
                })?;
            }

            ReaderPreferencesRequest::Update(preferences) => {
                self.preferences = preferences;
                self.preferences_dirty = true;

                self.persist_preferences().await;
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
                let entries = self.history.entries().to_vec();

                runtime.update(app, move |app, cx| {
                    app.apply_reading_history(entries, cx);
                })?;
            }
        }

        Ok(())
    }

    async fn service_frontlight_preferences_request(
        &mut self,
        request: FrontlightPreferencesRequest,
    ) {
        match request {
            FrontlightPreferencesRequest::Update(preferences) => {
                if self.frontlight_preferences == preferences {
                    return;
                }

                self.frontlight_preferences = preferences;
                self.frontlight_preferences_dirty = true;
            }

            FrontlightPreferencesRequest::Persist => {
                self.persist_frontlight_preferences().await;
            }
        }
    }

    async fn persist_history(&mut self) -> bool {
        if !self.history_dirty {
            return true;
        }

        let Ok(bytes) = self.history.encode() else {
            return false;
        };

        if self
            .platform
            .save_state(READING_HISTORY_STATE, &bytes)
            .await
            .is_err()
        {
            return false;
        }

        self.history_dirty = false;

        true
    }

    async fn persist_preferences(&mut self) -> bool {
        if !self.preferences_dirty {
            return true;
        }

        let font_name = match self.preferences.text().font() {
            ReaderFont::Card(index) => self
                .font_families
                .get(usize::from(index))
                .map(FontFamily::name),
            ReaderFont::BuiltIn => None,
        };

        let Ok(bytes) = self.preferences.encode(font_name) else {
            return false;
        };

        if self
            .platform
            .save_state(READER_PREFERENCES_STATE, &bytes)
            .await
            .is_err()
        {
            return false;
        }

        self.preferences_dirty = false;

        true
    }

    async fn persist_frontlight_preferences(&mut self) -> bool {
        if !self.frontlight_preferences_dirty {
            return true;
        }

        let Ok(bytes) = self.frontlight_preferences.encode() else {
            return false;
        };

        if self
            .platform
            .save_state(FRONTLIGHT_PREFERENCES_STATE, &bytes)
            .await
            .is_err()
        {
            return false;
        }

        self.frontlight_preferences_dirty = false;

        true
    }

    async fn persist_clock_preferences(&mut self) -> bool {
        if !self.clock_preferences_dirty {
            return true;
        }

        let Ok(bytes) = self.clock_preferences.encode() else {
            return false;
        };

        if self
            .platform
            .save_state(CLOCK_PREFERENCES_STATE, &bytes)
            .await
            .is_err()
        {
            return false;
        }

        self.clock_preferences_dirty = false;

        true
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

                let ready = self.platform.enter_usb_drive().await.is_ok();

                runtime.update(app, move |app, cx| {
                    app.apply_usb_drive_result(ready, cx);
                })?;
            }
        }

        Ok(())
    }
}
