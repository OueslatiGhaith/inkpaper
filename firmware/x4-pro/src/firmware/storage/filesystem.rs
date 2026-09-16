use core::fmt::Write;

use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};
use defmt::{debug, info, warn};
use hadris_fat::r#async::{DirectoryEntry, FatVolume, FileEntry};
use hadris_io::r#async::{Read as HadrisRead, Seek as HadrisSeek};

use crate::firmware::storage::{StorageEntry, history::INKPAPER_DIRECTORY_NAME};

const LOGGED_FILE_NAME_BYTES: usize = 256;

pub(super) async fn list_root<D>(filesystem: &FatVolume<D>) -> bool
where
    D: HadrisRead + HadrisSeek<Error = <D as HadrisRead>::Error>,
{
    debug!("listing filesystem root");

    let root = filesystem.root_dir();

    let mut entries = root.entries();

    let mut count = 0usize;

    loop {
        let Some(result) = entries.next_entry().await else {
            break;
        };

        let entry = match result {
            Ok(DirectoryEntry::Entry(entry)) => entry,
            Err(error) => {
                warn!("root directory read failed: {:?}", error);
                return false;
            }
        };

        let name = logged_entry_name(&entry);

        if entry.is_directory() {
            debug!("root entry kind=directory name={}", name.as_str());
        } else {
            debug!(
                "root entry kind=file name={} bytes={}",
                name.as_str(),
                entry.len() as u64,
            );
        }

        count += 1;
    }

    info!("root directory listed entries={}", count);

    true
}

pub(super) async fn list_directory<D>(
    filesystem: &FatVolume<D>,
    path: &str,
) -> Option<Vec<StorageEntry>>
where
    D: HadrisRead + HadrisSeek<Error = <D as HadrisRead>::Error>,
{
    debug!("listing directory path={}", path);

    let directory = if path == "/" {
        filesystem.root_dir()
    } else {
        match filesystem.open_dir_path(path).await {
            Ok(dir) => dir,

            Err(error) => {
                warn!("directory open failed path={} error={:?}", path, error);

                return None;
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

                return None;
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

    Some(output)
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

fn logged_entry_name(entry: &FileEntry) -> heapless::String<LOGGED_FILE_NAME_BYTES> {
    let mut output = heapless::String::new();

    if let Some(name) = entry.long_name() {
        for character in name.chars() {
            let required = character.len_utf8();

            if output.len() + required > LOGGED_FILE_NAME_BYTES - 3 {
                let _ = output.push_str("...");

                break;
            }

            let _ = output.push(character);
        }

        return output;
    }

    if let Ok(name) = entry.short_name().try_as_str() {
        let _ = output.push_str(name);

        return output;
    }

    let _ = write!(output, "<OEM {:02x?}>", entry.short_name().raw_bytes());

    output
}
