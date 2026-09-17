use std::path::{Path, PathBuf};

use futures_lite::future;
use inkpaper_app::{
    ReaderChapter, ReaderChapterDirection, ReaderDocument, ReaderSession, ReadingHistory,
    ReadingHistoryEntry, SpineIndex,
};

use crate::host_epub::HostFileSource;

pub(super) struct SimulatorReaderService {
    session: Option<ReaderSession<HostFileSource>>,
    history: ReadingHistory,
    history_path: PathBuf,
}

impl SimulatorReaderService {
    pub(super) fn new() -> Self {
        let history_path = simulator_history_path();

        let history = std::fs::read(&history_path)
            .ok()
            .and_then(|bytes| ReadingHistory::decode(&bytes).ok())
            .unwrap_or_default();

        Self {
            session: None,
            history,
            history_path,
        }
    }

    pub(super) fn open_document(&mut self, path: String) -> Option<ReaderDocument> {
        let reuse = match self.session.as_ref() {
            Some(session) => session.path() == path,

            None => false,
        };

        if !reuse {
            self.session = None;

            let source = simulator_epub_source(&path)?;

            let session = future::block_on(ReaderSession::open(path.clone(), source)).ok()?;

            self.session = Some(session);
        }

        let resume = {
            let session = self.session.as_ref()?;

            self.history.resume_position(&path, session.identifier())
        };

        future::block_on(self.session.as_mut()?.load_document_at(resume)).ok()
    }

    pub(super) fn load_adjacent_chapter(
        &mut self,
        path: &str,
        from: SpineIndex,
        direction: ReaderChapterDirection,
    ) -> Option<ReaderChapter> {
        let session = self.session.as_mut()?;

        if session.path() != path {
            return None;
        }

        future::block_on(session.load_adjacent_chapter(from, direction))
            .ok()
            .flatten()
    }

    pub(super) fn update_progress(&mut self, entry: ReadingHistoryEntry) {
        self.history.record(entry);

        let Ok(encoded) = self.history.encode() else {
            return;
        };

        let temporary = self.history_path.with_extension("tmp");

        if std::fs::write(&temporary, encoded).is_err() {
            return;
        }

        let _ = std::fs::rename(temporary, &self.history_path);
    }

    pub(super) fn reading_history(&self) -> Vec<ReadingHistoryEntry> {
        self.history.entries().to_vec()
    }
}

fn simulator_history_path() -> PathBuf {
    if let Some(path) = std::env::var_os("INKPAPER_SIM_HISTORY") {
        return PathBuf::from(path);
    }

    let directory = std::env::temp_dir()
        .join("inkpaper-simulator")
        .join(".inkpaper");

    std::fs::create_dir_all(&directory).expect("simulator state directory must be creatable");

    directory.join("reading-history.dat")
}

fn simulator_epub_source(path: &str) -> Option<HostFileSource> {
    let file_name = path.strip_prefix("/Fixtures/")?;

    let relative = Path::new(file_name);

    if relative.components().count() != 1 {
        return None;
    }

    if relative
        .extension()
        .and_then(|extension| extension.to_str())
        != Some("epub")
    {
        return None;
    }

    let host_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(relative);

    HostFileSource::open(&host_path).ok()
}
