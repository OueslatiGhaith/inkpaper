use alloc::vec::Vec;

use defmt::{info, warn};
use hadris_fat::r#async::{FatVolume, FatVolumeWriteExt};
use hadris_io::r#async::{Read as HadrisRead, Seek as HadrisSeek, Write as HadrisWrite};
use inkpaper_app::ReaderPreferences;

const INKPAPER_DIRECTORY_NAME: &str = ".inkpaper";
const INKPAPER_DIRECTORY_PATH: &str = "/.inkpaper";

const READER_PREFERENCES_FILE_NAME: &str = "reader-preferences.dat";
const READER_PREFERENCES_PATH: &str = "/.inkpaper/reader-preferences.dat";

const MAX_READER_PREFERENCES_BYTES: usize = 256;

pub(super) async fn load_reader_preferences<D>(filesystem: &FatVolume<D>) -> ReaderPreferences
where
    D: HadrisRead + HadrisSeek<Error = <D as HadrisRead>::Error>,
{
    let mut reader = match filesystem.open_file_path(READER_PREFERENCES_PATH).await {
        Ok(reader) => reader,
        Err(hadris_fat::Error::EntryNotFound) => return ReaderPreferences::default(),
        Err(error) => {
            warn!("reader preferences open failed error={:?}", error);
            return ReaderPreferences::default();
        }
    };

    let size = reader.size() as usize;

    if size == 0 || size > MAX_READER_PREFERENCES_BYTES {
        if size > MAX_READER_PREFERENCES_BYTES {
            warn!("reader preferences too large bytes={}", size);
        }

        return ReaderPreferences::default();
    }

    let mut bytes = Vec::new();
    bytes.resize(size, 0);

    let mut read = 0usize;

    while read < bytes.len() {
        let count = match reader.read(&mut bytes[read..]).await {
            Ok(count) => count,

            Err(error) => {
                warn!("reader preferences read failed error={:?}", error);
                return ReaderPreferences::default();
            }
        };

        if count == 0 {
            warn!("reader preferences ended unexpectedly");
            return ReaderPreferences::default();
        }

        read += count;
    }

    match ReaderPreferences::decode(&bytes) {
        Ok(preferences) => preferences,
        Err(_) => {
            warn!("reader preferences decode failed");
            ReaderPreferences::default()
        }
    }
}

pub(super) async fn save_reader_preferences<D>(
    filesystem: &FatVolume<D>,
    preferences: ReaderPreferences,
) -> bool
where
    D: HadrisRead
        + HadrisWrite<Error = <D as HadrisRead>::Error>
        + HadrisSeek<Error = <D as HadrisRead>::Error>,
{
    let bytes = match preferences.encode() {
        Ok(bytes) => bytes,
        Err(_) => {
            warn!("reader preferences encode failed");
            return false;
        }
    };

    if bytes.len() > MAX_READER_PREFERENCES_BYTES {
        warn!("encoded reader preferences too large bytes={}", bytes.len());
        return false;
    }

    let inkpaper_directory = match filesystem.open_dir_path(INKPAPER_DIRECTORY_PATH).await {
        Ok(directory) => directory,

        Err(hadris_fat::Error::EntryNotFound) => {
            let root = filesystem.root_dir();

            match filesystem.create_dir(&root, INKPAPER_DIRECTORY_NAME).await {
                Ok(directory) => directory,
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

    let entry = match filesystem.open_path(READER_PREFERENCES_PATH).await {
        Ok(entry) => entry,
        Err(hadris_fat::Error::EntryNotFound) => {
            match filesystem
                .create_file(&inkpaper_directory, READER_PREFERENCES_FILE_NAME)
                .await
            {
                Ok(entry) => entry,
                Err(error) => {
                    warn!("reader preferences create failed error={:?}", error);
                    return false;
                }
            }
        }

        Err(error) => {
            warn!("reader preferences lookup failed error={:?}", error);
            return false;
        }
    };

    let mut writer = match filesystem.write_file(&entry) {
        Ok(writer) => writer,
        Err(error) => {
            warn!("reader preferences writer open failed error={:?}", error);
            return false;
        }
    };

    let written = match writer.write(&bytes).await {
        Ok(written) => written,
        Err(error) => {
            warn!("reader preferences write failed error={:?}", error);
            return false;
        }
    };

    if written != bytes.len() {
        warn!(
            "reader preferences short write expected={} actual={}",
            bytes.len(),
            written,
        );

        return false;
    }

    if let Err(error) = writer.finish().await {
        warn!("reader preferences finish failed error={:?}", error);

        return false;
    }

    info!(
        "reader preferences saved path={} font_size={}",
        READER_PREFERENCES_PATH,
        preferences.font_size(),
    );

    true
}
