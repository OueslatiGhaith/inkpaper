use alloc::{
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use defmt::{debug, warn};
use hadris_fat::r#async::{DirectoryEntry, FatVolume, FileEntry};
use hadris_io::r#async::{Read as HadrisRead, Seek as HadrisSeek};

use crate::firmware::storage::{StorageEntry, StorageError, state::INKPAPER_DIRECTORY_NAME};

pub(super) async fn list_directory<D>(
    filesystem: &FatVolume<D>,
    path: &str,
) -> Result<Vec<StorageEntry>, StorageError>
where
    D: HadrisRead + HadrisSeek<Error = <D as HadrisRead>::Error>,
{
    debug!("listing directory path={}", path);

    let directory = if path == "/" {
        filesystem.root_dir()
    } else {
        match filesystem.open_dir_path(path).await {
            Ok(directory) => directory,

            Err(error) => {
                warn!("directory open failed path={} error={:?}", path, error);

                return Err(StorageError::Io);
            }
        }
    };

    let mut iterator = directory.entries();
    let mut output = Vec::new();

    loop {
        let Some(result) = iterator.next_entry().await else {
            break;
        };

        let entry = match result {
            Ok(DirectoryEntry::Entry(entry)) => entry,

            Err(error) => {
                warn!("directory read failed path={} error={:?}", path, error);
                return Err(StorageError::Io);
            }
        };

        let name = owned_entry_name(&entry);

        if name == "." || name == ".." {
            continue;
        }

        if path == "/" && name.eq_ignore_ascii_case(INKPAPER_DIRECTORY_NAME) {
            continue;
        }

        output.push(StorageEntry::new(name, entry.is_directory()));
    }

    debug!("directory listed path={} entries={}", path, output.len());

    Ok(output)
}

pub(super) async fn read_file<D>(
    filesystem: &FatVolume<D>,
    path: &str,
    max_bytes: usize,
) -> Result<Vec<u8>, StorageError>
where
    D: HadrisRead + HadrisSeek<Error = <D as HadrisRead>::Error>,
{
    let mut reader = filesystem.open_file_path(path).await.map_err(|error| {
        warn!("file open failed path={} error={:?}", path, error);

        StorageError::Io
    })?;

    let size = reader.size() as usize;

    if size > max_bytes {
        warn!(
            "file too large path={} bytes={} limit={}",
            path, size, max_bytes
        );

        return Err(StorageError::FileTooLarge);
    }

    let mut bytes = vec![0; size];
    let mut read = 0usize;

    while read < bytes.len() {
        let count = reader.read(&mut bytes[read..]).await.map_err(|error| {
            warn!("file read failed path={} error={:?}", path, error);

            StorageError::Io
        })?;

        if count == 0 {
            return Err(StorageError::UnexpectedEof);
        }

        read += count;
    }

    debug!("file read path={} bytes={}", path, size);

    Ok(bytes)
}

fn owned_entry_name(entry: &FileEntry) -> String {
    if let Some(name) = entry.long_name() {
        return name.to_string();
    }

    if let Ok(name) = entry.short_name().try_as_str() {
        return String::from(name);
    }

    format!("<OEM {:02x?}>", entry.short_name().raw_bytes())
}
