use alloc::vec::Vec;
use defmt::{info, warn};
use hadris_fat::r#async::{FatVolume, FatVolumeWriteExt};
use hadris_io::r#async::{Read as HadrisRead, Seek as HadrisSeek, Write as HadrisWrite};
use inkpaper_app::ReadingHistory;

pub(super) const INKPAPER_DIRECTORY_NAME: &str = ".inkpaper";

const INKPAPER_DIRECTORY_PATH: &str = "/.inkpaper";
const READING_HISTORY_FILE_NAME: &str = "reading-history.dat";
const READING_HISTORY_PATH: &str = "/.inkpaper/reading-history.dat";

const MAX_READING_HISTORY_BYTES: usize = 64 * 1024;

pub(super) async fn load_reading_history<D>(filesystem: &FatVolume<D>) -> ReadingHistory
where
    D: HadrisRead + HadrisSeek<Error = <D as HadrisRead>::Error>,
{
    let mut reader = match filesystem.open_file_path(READING_HISTORY_PATH).await {
        Ok(reader) => reader,

        Err(hadris_fat::Error::EntryNotFound) => {
            return ReadingHistory::default();
        }

        Err(error) => {
            warn!("reading history open failed error={:?}", error);

            return ReadingHistory::default();
        }
    };

    let size = reader.size() as usize;

    if size == 0 {
        return ReadingHistory::default();
    }

    if size > MAX_READING_HISTORY_BYTES {
        warn!("reading history too large bytes={}", size);

        return ReadingHistory::default();
    }

    let mut bytes = Vec::new();

    bytes.resize(size, 0);

    let mut read = 0usize;

    while read < bytes.len() {
        let count = match reader.read(&mut bytes[read..]).await {
            Ok(count) => count,

            Err(error) => {
                warn!("reading history read failed error={:?}", error);

                return ReadingHistory::default();
            }
        };

        if count == 0 {
            warn!("reading history ended unexpectedly");

            return ReadingHistory::default();
        }

        read += count;
    }

    match ReadingHistory::decode(&bytes) {
        Ok(history) => history,

        Err(_) => {
            warn!("reading history decode failed");

            ReadingHistory::default()
        }
    }
}

pub(super) async fn save_reading_history<D>(
    filesystem: &FatVolume<D>,
    history: &ReadingHistory,
) -> bool
where
    D: HadrisRead
        + HadrisWrite<Error = <D as HadrisRead>::Error>
        + HadrisSeek<Error = <D as HadrisRead>::Error>,
{
    let bytes = match history.encode() {
        Ok(bytes) => bytes,

        Err(_) => {
            warn!("reading history encode failed");

            return false;
        }
    };

    if bytes.len() > MAX_READING_HISTORY_BYTES {
        warn!("encoded reading history too large bytes={}", bytes.len());

        return false;
    }

    let inkpaper_directory = match filesystem.open_dir_path(INKPAPER_DIRECTORY_PATH).await {
        Ok(directory) => directory,

        Err(hadris_fat::Error::EntryNotFound) => {
            let root = filesystem.root_dir();

            match filesystem.create_dir(&root, INKPAPER_DIRECTORY_NAME).await {
                Ok(directory) => {
                    info!(
                        "created InkPaper storage directory path={}",
                        INKPAPER_DIRECTORY_PATH,
                    );

                    directory
                }

                Err(error) => {
                    warn!("InkPaper storage directory create failed error={:?}", error);

                    return false;
                }
            }
        }

        Err(error) => {
            warn!("InkPaper storage directory open failed error={:?}", error);

            return false;
        }
    };

    let entry = match filesystem.open_path(READING_HISTORY_PATH).await {
        Ok(entry) => entry,

        Err(hadris_fat::Error::EntryNotFound) => {
            match filesystem
                .create_file(&inkpaper_directory, READING_HISTORY_FILE_NAME)
                .await
            {
                Ok(entry) => entry,

                Err(error) => {
                    warn!("reading history create failed error={:?}", error);

                    return false;
                }
            }
        }

        Err(error) => {
            warn!("reading history lookup failed error={:?}", error);

            return false;
        }
    };

    let mut writer = match filesystem.write_file(&entry) {
        Ok(writer) => writer,

        Err(error) => {
            warn!("reading history writer open failed error={:?}", error);

            return false;
        }
    };

    let written = match writer.write(&bytes).await {
        Ok(written) => written,

        Err(error) => {
            warn!("reading history write failed error={:?}", error);

            return false;
        }
    };

    if written != bytes.len() {
        warn!(
            "reading history short write expected={} actual={}",
            bytes.len(),
            written,
        );

        return false;
    }

    if let Err(error) = writer.finish().await {
        warn!("reading history finish failed error={:?}", error);

        return false;
    }

    info!(
        "reading history saved path={} entries={}",
        READING_HISTORY_PATH,
        history.entries().len(),
    );

    true
}
