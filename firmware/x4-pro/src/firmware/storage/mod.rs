use aligned::{A4, Aligned};
use alloc::{string::String, vec::Vec};
use block_device_driver::BlockDevice as RawBlockDevice;
use defmt::{debug, error, info, warn};
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel, signal::Signal,
};
use embassy_time::{Delay, Duration, Timer};
use esp_hal::{
    peripherals::{GPIO40, GPIO41, GPIO42, SDHOST},
    sdmmc::{Config, SdHostController, SlotConfig},
};
use hadris_fat::r#async::FatVolume;
use hadris_io::r#async::{Read as HadrisRead, Seek as HadrisSeek, Write as HadrisWrite};
use inkpaper_app::{
    ReaderChapter, ReaderChapterDirection, ReaderDocument, ReaderSession, ReadingProgress,
    SpineIndex,
};
use sdio::{BlockDevice, MmcBus, sd::Card};

use crate::firmware::{
    power::SdPower,
    storage::{
        epub::{FatEpubSource, open_reader_session},
        filesystem::{list_directory, list_root},
        history::{load_reading_history, save_reading_history},
        partition::{SECTOR_SIZE, SdPartition, find_fat_partition},
    },
};

mod epub;
mod filesystem;
mod history;
mod partition;

const TARGET_FREQUENCY_HZ: u32 = 40_000_000;
const MOUNT_ATTEMPTS: usize = 4;

const POWER_OFF_MS: u64 = 80;
const POWER_SETTLE_MS: u64 = 120;

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
    fn new(name: String, is_directory: bool) -> Self {
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

#[embassy_executor::task]
pub async fn storage_task(
    sdhost: SDHOST<'static>,
    clk: GPIO41<'static>,
    cmd: GPIO42<'static>,
    data0: GPIO40<'static>,
    mut sd_power: SdPower<'static>,
) {
    info!("storage service starting");
    debug!("SDMMC configuration slot=1 clk_gpio=41 cmd_gpio=42 data0_gpio=40 enable_gpio=5");

    let controller = match SdHostController::new(sdhost, Config::default()) {
        Ok(controller) => controller,
        Err(error) => {
            error!("SDHOST initialization failed: {:?}", error);

            serve_unavailable(&mut sd_power).await;

            return;
        }
    };

    let mut slot = match controller.slot::<1>(SlotConfig::default()) {
        Ok(slot) => slot
            .with_clk(clk)
            .with_cmd(cmd)
            .with_data0(data0)
            .into_async(),
        Err(error) => {
            error!("SD slot initialization failed: {:?}", error);

            serve_unavailable(&mut sd_power).await;

            return;
        }
    };

    run_storage(&mut slot, &mut sd_power).await;
}

async fn run_storage<B>(bus: &mut B, sd_power: &mut SdPower<'_>)
where
    B: MmcBus,
{
    for attempt in 1..=MOUNT_ATTEMPTS {
        info!("SD mount attempt {}/{}", attempt, MOUNT_ATTEMPTS);

        power_cycle_sd(sd_power).await;

        // this BlockDevice is intentionally reconstructed for every attempt.
        // `sdio::BlockDevice` retains transfer-error state after a failed read. A fresh
        // object means a failed sector 0 test cannot poison the following power-cycle.
        let mut card: BlockDevice<Card, _, _, SECTOR_SIZE> =
            match BlockDevice::new(&mut *bus, Delay, TARGET_FREQUENCY_HZ).await {
                Ok(card) => {
                    info!("SD card initialized at {} Hz", card.freq());

                    card
                }

                Err(error) => {
                    warn!("SD card initialization failed: {:?}", error);

                    continue;
                }
            };

        let mut sector_zero = [Aligned::<A4, _>([0u8; SECTOR_SIZE])];

        if let Err(error) = card.read(0, &mut sector_zero).await {
            warn!("SD sector 0 read failed: {:?}", error);

            continue;
        }

        debug!("SD sector 0 read succeeded");

        match card.size().await {
            Ok(size) => info!("SD capacity: {} bytes / {} MiB", size, size / 1024 / 1024),
            Err(error) => warn!("could not read SD capacity: {:?}", error),
        }

        let Some(partition) = find_fat_partition(&sector_zero[0]) else {
            warn!("no supported FAT partition found");
            continue;
        };

        info!(
            "FAT partition index={} type={:#04x} first_lba={} sectors={}",
            partition.index, partition.partition_type, partition.first_lba, partition.sectors,
        );

        let partition_device = SdPartition::new(&mut card, partition.first_lba, partition.sectors);

        let filesystem = match FatVolume::open(partition_device).await {
            Ok(filesystem) => filesystem,
            Err(error) => {
                warn!("FAT mount failed: {:?}", error);
                continue;
            }
        };

        let volume = filesystem.volume_info();

        info!(
            "FAT filesystem mounted oem={} label={} type={}",
            volume.oem_name(),
            volume.volume_label(),
            volume.fs_type_str(),
        );

        // from this point until shutdown, GPIO5 remains LOW and this task owns the complete
        // SD -> block device -> partition -> FAT stack.
        READY.signal(true);

        serve_filesystem(&filesystem).await;

        // Command::Shutdown has been received.
        // drop the filesystem before the block device, then drive GPIO5 HIGH. The shutdown
        // acknowledgement is not sent until that electrical state is established.
        drop(filesystem);
        drop(card);

        sd_power.disable();

        info!("storage shut down; SD power off");

        SHUTDOWN_DONE.signal(());

        // keep ownership of the GPIO5 Output alive while main latches the RTC pad
        // and the power task enters deep sleep.
        hold_forever().await;

        return;
    }

    serve_unavailable(sd_power).await;
}

async fn serve_filesystem<'a, D>(filesystem: &'a FatVolume<D>)
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
                            warn!("EPUB reader load failed path={}", path.as_str());

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
                debug!("storage shutdown requested");

                // drop the FileReader before returning to run_storage(), which then drops
                // the FAT volume and block device.
                drop(reader_session);

                if reading_history_dirty
                    && !save_reading_history(filesystem, &reading_history).await
                {
                    warn!("reading history was not persisted");
                }

                return;
            }
        }
    }
}

async fn serve_unavailable(sd_power: &mut SdPower<'_>) {
    sd_power.disable();

    error!("storage unavailable");

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
                // GPIO5 is already HIGH, but establish it explicitly before acknowledging
                // the power path.
                sd_power.disable();
                SHUTDOWN_DONE.signal(());
                hold_forever().await;

                return;
            }
        }
    }
}

async fn power_cycle_sd(sd_power: &mut SdPower<'_>) {
    sd_power.disable();
    Timer::after(Duration::from_millis(POWER_OFF_MS)).await;

    sd_power.enable();
    Timer::after(Duration::from_millis(POWER_SETTLE_MS)).await;
}

async fn hold_forever() {
    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}
