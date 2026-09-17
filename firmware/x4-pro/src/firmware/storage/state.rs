use alloc::{format, string::String, vec::Vec};

use defmt::{info, warn};
use hadris_fat::r#async::{FatVolume, FatVolumeWriteExt};
use hadris_io::r#async::{Read as HadrisRead, Seek as HadrisSeek, Write as HadrisWrite};

use super::types::{MAX_STATE_BYTES, StorageError};

pub(super) const INKPAPER_DIRECTORY_NAME: &str = ".inkpaper";

const INKPAPER_DIRECTORY_PATH: &str = "/.inkpaper";
const MAX_STATE_NAME_BYTES: usize = 64;

pub(super) async fn load_state<D>(
    filesystem: &FatVolume<D>,
    name: &str,
    max_bytes: usize,
) -> Result<Option<Vec<u8>>, StorageError>
where
    D: HadrisRead + HadrisSeek<Error = <D as HadrisRead>::Error>,
{
    if max_bytes > MAX_STATE_BYTES {
        return Err(StorageError::StateTooLarge);
    }

    let path = state_path(name)?;

    let mut reader = match filesystem.open_file_path(&path).await {
        Ok(reader) => reader,
        Err(hadris_fat::Error::EntryNotFound) => return Ok(None),
        Err(error) => {
            warn!("state open failed name={} error={:?}", name, error);
            return Err(StorageError::Io);
        }
    };

    let size = reader.size() as usize;

    if size > max_bytes {
        warn!(
            "state file too large name={} bytes={} limit={}",
            name, size, max_bytes,
        );

        return Err(StorageError::StateTooLarge);
    }

    let mut bytes = Vec::new();
    bytes.resize(size, 0);

    let mut read = 0usize;

    while read < bytes.len() {
        let count = reader.read(&mut bytes[read..]).await.map_err(|error| {
            warn!("state read failed name={} error={:?}", name, error);

            StorageError::Io
        })?;

        if count == 0 {
            return Err(StorageError::UnexpectedEof);
        }

        read += count;
    }

    Ok(Some(bytes))
}

pub(super) async fn save_state<D>(
    filesystem: &FatVolume<D>,
    name: &str,
    bytes: &[u8],
) -> Result<(), StorageError>
where
    D: HadrisRead
        + HadrisWrite<Error = <D as HadrisRead>::Error>
        + HadrisSeek<Error = <D as HadrisRead>::Error>,
{
    if bytes.len() > MAX_STATE_BYTES {
        return Err(StorageError::StateTooLarge);
    }

    let path = state_path(name)?;

    let inkpaper_directory = match filesystem.open_dir_path(INKPAPER_DIRECTORY_PATH).await {
        Ok(directory) => directory,

        Err(hadris_fat::Error::EntryNotFound) => {
            let root = filesystem.root_dir();

            filesystem
                .create_dir(&root, INKPAPER_DIRECTORY_NAME)
                .await
                .map_err(|error| {
                    warn!("InkPaper state directory create failed error={:?}", error);

                    StorageError::Io
                })?
        }

        Err(error) => {
            warn!("InkPaper state directory open failed error={:?}", error);
            return Err(StorageError::Io);
        }
    };

    let entry = match filesystem.open_path(&path).await {
        Ok(entry) => entry,

        Err(hadris_fat::Error::EntryNotFound) => filesystem
            .create_file(&inkpaper_directory, name)
            .await
            .map_err(|error| {
                warn!("state file create failed name={} error={:?}", name, error);

                StorageError::Io
            })?,

        Err(error) => {
            warn!("state lookup failed name={} error={:?}", name, error);

            return Err(StorageError::Io);
        }
    };

    let mut writer = filesystem.write_file(&entry).map_err(|error| {
        warn!("state writer open failed name={} error={:?}", name, error);

        StorageError::Io
    })?;

    let mut written = 0usize;

    while written < bytes.len() {
        let count = writer.write(&bytes[written..]).await.map_err(|error| {
            warn!("state write failed name={} error={:?}", name, error);

            StorageError::Io
        })?;

        if count == 0 {
            return Err(StorageError::Io);
        }

        written += count;
    }

    writer.finish().await.map_err(|error| {
        warn!("state finish failed name={} error={:?}", name, error);

        StorageError::Io
    })?;

    info!("state saved name={} bytes={}", name, bytes.len());

    Ok(())
}

fn state_path(name: &str) -> Result<String, StorageError> {
    if !valid_state_name(name) {
        return Err(StorageError::InvalidStateName);
    }

    Ok(format!("{INKPAPER_DIRECTORY_PATH}/{name}"))
}

fn valid_state_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_STATE_NAME_BYTES
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
}
