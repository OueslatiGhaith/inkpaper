use alloc::{string::String, vec::Vec};

use inkpaper_epub::EpubSource;
use inkpaper_ui::{Entity, EntityAccessError, ResourceRuntimeApi, RuntimeApi};

use crate::{
    BrowseEntry, BrowseListing, BrowseRequest, FileTransferRequest, FrontlightPreferences,
    FrontlightPreferencesRequest, FrontlightSetting, InkPaperApp, ReaderPreferences,
    ReaderPreferencesRequest, ReaderRequest, ReaderSession, ReadingHistory, ReadingHistoryRequest,
};

const READING_HISTORY_STATE: &str = "reading-history.dat";
const READER_PREFERENCES_STATE: &str = "reader-preferences.dat";
const FRONTLIGHT_PREFERENCES_STATE: &str = "frontlight-preferences.dat";
const MAX_FRONTLIGHT_PREFERENCES_BYTES: usize = 64;

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

    async fn set_frontlight(&mut self, setting: FrontlightSetting) -> Result<(), Self::Error>;

    async fn load_state(
        &mut self,
        name: &str,
        max_bytes: usize,
    ) -> Result<Option<Vec<u8>>, Self::Error>;

    async fn save_state(&mut self, name: &str, bytes: &[u8]) -> Result<(), Self::Error>;

    async fn enter_usb_drive(&mut self) -> Result<(), Self::Error>;
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

    history: ReadingHistory,
    preferences: ReaderPreferences,
    frontlight_preferences: FrontlightPreferences,

    history_dirty: bool,
    preferences_dirty: bool,
    frontlight_preferences_dirty: bool,

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

            history: ReadingHistory::default(),
            preferences: ReaderPreferences::default(),
            frontlight_preferences: FrontlightPreferences::default(),

            history_dirty: false,
            preferences_dirty: false,
            frontlight_preferences_dirty: false,

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
            let frontlight_request = runtime.update(app, |app, _| app.take_frontlight_request())?;

            if let Some(setting) = frontlight_request {
                let _ = self.platform.set_frontlight(setting).await;
                continue;
            }

            let frontlight_preferences_request =
                runtime.update(app, |app, _| app.take_frontlight_preferences_request())?;

            if let Some(request) = frontlight_preferences_request {
                self.service_frontlight_preferences_request(request).await;
                continue;
            }

            let preferences_request =
                runtime.update(app, |app, _| app.take_reader_preferences_request())?;

            if let Some(request) = preferences_request {
                self.service_reader_preferences_request(runtime, app, request)
                    .await?;

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

        history_saved && preferences_saved && frontlight_preferences_saved
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

        self.history = self.load_history().await;
        self.preferences = self.load_preferences().await;
        self.frontlight_preferences = self.load_frontlight_preferences().await;

        let entries = self.history.entries().to_vec();
        let preferences = self.preferences;
        let frontlight_preferences = self.frontlight_preferences;

        runtime.update(app, move |app, cx| {
            app.apply_frontlight_preferences(frontlight_preferences, cx);
            app.apply_reader_preferences(preferences, cx);
            app.apply_reading_history(entries, cx);
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

        ReaderPreferences::decode(&bytes).unwrap_or_default()
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
            ReaderRequest::OpenEpub { path, font_size } => {
                self.open_document(runtime, app, path, font_size).await?;
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

            ReaderRequest::RepaginateChapter {
                path,
                spine,
                font_size,
            } => {
                let chapter = match self.reader_session.as_mut() {
                    Some(session) if session.path() == path => session
                        .repaginate_chapter(spine, font_size)
                        .await
                        .ok()
                        .flatten(),

                    _ => None,
                };

                match chapter {
                    Some(mut chapter) => {
                        chapter.register_images(runtime);

                        runtime.update(app, move |app, cx| {
                            app.apply_reader_repagination(path, spine, font_size, chapter, cx);
                        })?;
                    }

                    None => {
                        runtime.update(app, move |app, _| {
                            app.finish_reader_repagination_request(path, spine, font_size);
                        })?;
                    }
                }
            }

            ReaderRequest::JumpTo {
                path,
                spine,
                anchor,
            } => {
                let target = match self.reader_session.as_mut() {
                    Some(session) if session.path() == path => session
                        .load_chapter_at(spine, anchor.as_deref())
                        .await
                        .ok()
                        .flatten(),

                    _ => None,
                };

                match target {
                    Some((mut chapter, page_index)) => {
                        chapter.register_images(runtime);

                        runtime.update(app, move |app, cx| {
                            app.apply_reader_jump(path, spine, anchor, chapter, page_index, cx);
                        })?;
                    }

                    None => {
                        runtime.update(app, move |app, _| {
                            app.finish_reader_jump_request(path, spine, anchor);
                        })?;
                    }
                }
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
        font_size: u16,
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

            if !session.set_font_size(font_size) {
                None
            } else {
                let resume = self.history.resume_position(&path, session.identifier());

                session.load_document_at(resume).await.ok()
            }
        };

        match document {
            Some(mut document) => {
                document.register_images(runtime);

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

        let Ok(bytes) = self.preferences.encode() else {
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
