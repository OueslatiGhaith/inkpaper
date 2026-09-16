use alloc::{string::String, vec::Vec};
use defmt::{debug, info, warn};
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel, signal::Signal,
};
use hadris_fat::r#async::FatVolume;
use hadris_io::r#async::{Read as HadrisRead, Seek as HadrisSeek, Write as HadrisWrite};
use inkpaper_app::{
    ReaderChapter, ReaderChapterDirection, ReaderDocument, ReaderSession, ReadingProgress,
    SpineIndex,
};

use crate::firmware::storage::{
    epub::{FatEpubSource, open_reader_session},
    filesystem::{list_directory, list_root},
    history::{load_reading_history, save_reading_history},
};

const COMMAND_CAPACITY: usize = 4;

static COMMANDS: Channel<CriticalSectionRawMutex, Command, COMMAND_CAPACITY> = Channel::new();
static READY: Signal<CriticalSectionRawMutex, bool> = Signal::new();
static ROOT_LIST_DONE: Signal<CriticalSectionRawMutex, bool> = Signal::new();
static SHUTDOWN_DONE: Signal<CriticalSectionRawMutex, ()> = Signal::new();
static DIRECTORY_LIST_DONE: Signal<CriticalSectionRawMutex, Option<Vec<StorageEntry>>> =
    Signal::new();
static EPUB_DOCUMENT_DONE: Signal<CriticalSectionRawMutex, Option<ReaderDocument>> = Signal::new();
static EPUB_CHAPTER_DONE: Signal<CriticalSectionRawMutex, Option<ReaderChapter>> = Signal::new();
static READING_HISTORY_DONE: Signal<CriticalSectionRawMutex, Option<Vec<ReadingProgress>>> =
    Signal::new();

#[derive(Debug)]
enum Command {
    ListRoot,
    ListDirectory(String),
    LoadEpub(String),
    LoadEpubChapter {
        path: String,
        from: SpineIndex,
        direction: ReaderChapterDirection,
    },
    LoadReadingHistory,
    UpdateReadingProgress(ReadingProgress),
    Shutdown,
}

#[derive(Debug)]
pub struct StorageEntry {
    name: String,
    is_directory: bool,
}

impl StorageEntry {
    pub(super) fn new(name: String, is_directory: bool) -> Self {
        Self { name, is_directory }
    }

    pub fn into_parts(self) -> (String, bool) {
        (self.name, self.is_directory)
    }
}

pub async fn wait_ready() -> bool {
    READY.wait().await
}

pub async fn list_root_and_wait() -> bool {
    ROOT_LIST_DONE.reset();
    COMMANDS.send(Command::ListRoot).await;
    ROOT_LIST_DONE.wait().await
}

pub async fn shutdown_and_wait() {
    SHUTDOWN_DONE.reset();
    COMMANDS.send(Command::Shutdown).await;
    SHUTDOWN_DONE.wait().await;
}

pub async fn list_directory_and_wait(path: &str) -> Option<Vec<StorageEntry>> {
    DIRECTORY_LIST_DONE.reset();
    COMMANDS
        .send(Command::ListDirectory(String::from(path)))
        .await;
    DIRECTORY_LIST_DONE.wait().await
}

pub async fn load_epub_document_and_wait(path: &str) -> Option<ReaderDocument> {
    EPUB_DOCUMENT_DONE.reset();
    COMMANDS.send(Command::LoadEpub(String::from(path))).await;
    EPUB_DOCUMENT_DONE.wait().await
}

pub async fn load_epub_chapter_and_wait(
    path: &str,
    from: SpineIndex,
    direction: ReaderChapterDirection,
) -> Option<ReaderChapter> {
    EPUB_CHAPTER_DONE.reset();
    COMMANDS
        .send(Command::LoadEpubChapter {
            path: String::from(path),
            from,
            direction,
        })
        .await;
    EPUB_CHAPTER_DONE.wait().await
}

pub async fn update_reading_progress(progress: ReadingProgress) {
    COMMANDS
        .send(Command::UpdateReadingProgress(progress))
        .await;
}

pub async fn reading_history_and_wait() -> Option<Vec<ReadingProgress>> {
    READING_HISTORY_DONE.reset();
    COMMANDS.send(Command::LoadReadingHistory).await;
    READING_HISTORY_DONE.wait().await
}

pub(super) fn signal_ready(ready: bool) {
    READY.signal(ready);
}

pub(super) fn signal_shutdown_done() {
    SHUTDOWN_DONE.signal(());
}

pub(super) async fn serve_filesystem<'a, D>(filesystem: &'a FatVolume<D>)
where
    D: HadrisRead
        + HadrisWrite<Error = <D as HadrisRead>::Error>
        + HadrisSeek<Error = <D as HadrisRead>::Error>,
{
    let mut reader_session: Option<ReaderSession<FatEpubSource<'a, D>>> = None;
    let mut reading_history = load_reading_history(filesystem).await;
    let mut reading_history_dirty = false;

    loop {
        match COMMANDS.receive().await {
            Command::ListRoot => {
                let success = list_root(filesystem).await;
                ROOT_LIST_DONE.signal(success);
            }

            Command::ListDirectory(path) => {
                let entries = list_directory(filesystem, &path).await;
                DIRECTORY_LIST_DONE.signal(entries);
            }

            Command::LoadEpub(path) => {
                let reuse_session = match reader_session.as_ref() {
                    Some(session) => session.path() == path,

                    None => false,
                };

                if !reuse_session {
                    // drop the old FAT FileReader before opening another book.
                    let _ = reader_session.take();

                    reader_session = open_reader_session(filesystem, path.clone()).await;
                }

                let resume = match reader_session.as_ref() {
                    Some(session) => reading_history.resume_position(&path, session.identifier()),
                    None => None,
                };

                let document = match reader_session.as_mut() {
                    Some(session) => match session.load_document_at(resume).await {
                        Ok(document) => {
                            info!(
                                "EPUB reader ready path={} spine={} page={} pages={}",
                                path.as_str(),
                                document.spine().get(),
                                document.opening_page_index().saturating_add(1),
                                document.page_count(),
                            );

                            Some(document)
                        }

                        Err(_) => {
                            warn!("EPUB reader load failed path={}", path.as_str(),);

                            None
                        }
                    },

                    None => None,
                };

                EPUB_DOCUMENT_DONE.signal(document);
            }

            Command::LoadEpubChapter {
                path,
                from,
                direction,
            } => {
                let chapter = match reader_session.as_mut() {
                    Some(session) if session.path() == path => {
                        match session.load_adjacent_chapter(from, direction).await {
                            Ok(chapter) => chapter,

                            Err(_) => {
                                warn!(
                                    "EPUB adjacent chapter load failed path={} from={}",
                                    path.as_str(),
                                    from.get(),
                                );

                                None
                            }
                        }
                    }

                    Some(_) => {
                        warn!(
                            "EPUB chapter request does not match active session path={}",
                            path.as_str(),
                        );

                        None
                    }

                    None => {
                        warn!(
                            "EPUB chapter request without active session path={}",
                            path.as_str(),
                        );

                        None
                    }
                };

                EPUB_CHAPTER_DONE.signal(chapter);
            }

            Command::LoadReadingHistory => {
                READING_HISTORY_DONE.signal(Some(reading_history.entries().to_vec()));
            }

            Command::UpdateReadingProgress(progress) => {
                reading_history.record(progress);
                reading_history_dirty = true;
            }

            Command::Shutdown => {
                debug!("storage shutdown requested",);

                // drop the FileReader before the FAT volume and block device are dropped.
                drop(reader_session);

                if reading_history_dirty
                    && !save_reading_history(filesystem, &reading_history).await
                {
                    warn!("reading history was not persisted",);
                }

                return;
            }
        }
    }
}

pub(super) async fn serve_unavailable_requests() {
    READY.signal(false);

    loop {
        match COMMANDS.receive().await {
            Command::ListRoot => ROOT_LIST_DONE.signal(false),
            Command::ListDirectory(_) => DIRECTORY_LIST_DONE.signal(None),
            Command::LoadEpub(_) => EPUB_DOCUMENT_DONE.signal(None),
            Command::LoadEpubChapter { .. } => EPUB_CHAPTER_DONE.signal(None),
            Command::LoadReadingHistory => READING_HISTORY_DONE.signal(None),
            Command::UpdateReadingProgress(_) => {}
            Command::Shutdown => {
                SHUTDOWN_DONE.signal(());
                return;
            }
        }
    }
}
