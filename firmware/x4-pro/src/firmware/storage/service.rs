use alloc::{string::String, vec::Vec};

use defmt::debug;
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel, signal::Signal,
};
use hadris_fat::r#async::FatVolume;
use hadris_io::r#async::{Read as HadrisRead, Seek as HadrisSeek, Write as HadrisWrite};

use super::{
    filesystem::{list_directory, list_root},
    random_access::OpenRandomAccessFile,
    state::{load_state, save_state},
    types::{
        MAX_RANDOM_ACCESS_READ_BYTES, MAX_STATE_BYTES, RandomAccessHandle, StorageEntry,
        StorageError,
    },
};

const COMMAND_CAPACITY: usize = 4;

static COMMANDS: Channel<CriticalSectionRawMutex, Command, COMMAND_CAPACITY> = Channel::new();

static READY: Signal<CriticalSectionRawMutex, bool> = Signal::new();

static ROOT_LIST_DONE: Signal<CriticalSectionRawMutex, bool> = Signal::new();

static SHUTDOWN_DONE: Signal<CriticalSectionRawMutex, ()> = Signal::new();

static USB_DRIVE_READY: Signal<CriticalSectionRawMutex, bool> = Signal::new();

static DIRECTORY_LIST_DONE: Signal<
    CriticalSectionRawMutex,
    Result<Vec<StorageEntry>, StorageError>,
> = Signal::new();

static RANDOM_ACCESS_OPEN_DONE: Signal<
    CriticalSectionRawMutex,
    Result<RandomAccessHandle, StorageError>,
> = Signal::new();

static RANDOM_ACCESS_READ_DONE: Signal<CriticalSectionRawMutex, Result<Vec<u8>, StorageError>> =
    Signal::new();

static STATE_LOAD_DONE: Signal<CriticalSectionRawMutex, Result<Option<Vec<u8>>, StorageError>> =
    Signal::new();

static STATE_SAVE_DONE: Signal<CriticalSectionRawMutex, Result<(), StorageError>> = Signal::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FilesystemExit {
    Shutdown,
    UsbDrive,
}

#[derive(Debug)]
enum Command {
    ListRoot,

    ListDirectory(String),

    OpenRandomAccess(String),

    ReadRandomAccess {
        handle: u32,
        offset: u64,
        len: usize,
    },

    LoadState {
        name: String,
        max_bytes: usize,
    },

    SaveState {
        name: String,
        bytes: Vec<u8>,
    },

    EnterUsbDrive,

    Shutdown,
}

pub async fn wait_ready() -> bool {
    READY.wait().await
}

pub async fn list_root_and_wait() -> bool {
    ROOT_LIST_DONE.reset();
    COMMANDS.send(Command::ListRoot).await;
    ROOT_LIST_DONE.wait().await
}

pub async fn list_directory_and_wait(path: &str) -> Result<Vec<StorageEntry>, StorageError> {
    DIRECTORY_LIST_DONE.reset();
    COMMANDS
        .send(Command::ListDirectory(String::from(path)))
        .await;
    DIRECTORY_LIST_DONE.wait().await
}

pub async fn open_random_access_and_wait(path: &str) -> Result<RandomAccessHandle, StorageError> {
    RANDOM_ACCESS_OPEN_DONE.reset();
    COMMANDS
        .send(Command::OpenRandomAccess(String::from(path)))
        .await;
    RANDOM_ACCESS_OPEN_DONE.wait().await
}

pub async fn read_random_access_and_wait(
    handle: u32,
    offset: u64,
    len: usize,
) -> Result<Vec<u8>, StorageError> {
    if len > MAX_RANDOM_ACCESS_READ_BYTES {
        return Err(StorageError::ReadTooLarge);
    }

    RANDOM_ACCESS_READ_DONE.reset();
    COMMANDS
        .send(Command::ReadRandomAccess {
            handle,
            offset,
            len,
        })
        .await;
    RANDOM_ACCESS_READ_DONE.wait().await
}

pub async fn load_state_and_wait(
    name: &str,
    max_bytes: usize,
) -> Result<Option<Vec<u8>>, StorageError> {
    if max_bytes > MAX_STATE_BYTES {
        return Err(StorageError::StateTooLarge);
    }

    STATE_LOAD_DONE.reset();
    COMMANDS
        .send(Command::LoadState {
            name: String::from(name),
            max_bytes,
        })
        .await;
    STATE_LOAD_DONE.wait().await
}

pub async fn save_state_and_wait(name: &str, bytes: &[u8]) -> Result<(), StorageError> {
    if bytes.len() > MAX_STATE_BYTES {
        return Err(StorageError::StateTooLarge);
    }

    STATE_SAVE_DONE.reset();
    COMMANDS
        .send(Command::SaveState {
            name: String::from(name),
            bytes: Vec::from(bytes),
        })
        .await;
    STATE_SAVE_DONE.wait().await
}

pub async fn shutdown_and_wait() {
    SHUTDOWN_DONE.reset();
    COMMANDS.send(Command::Shutdown).await;
    SHUTDOWN_DONE.wait().await;
}

pub async fn enter_usb_drive_and_wait() -> bool {
    USB_DRIVE_READY.reset();
    COMMANDS.send(Command::EnterUsbDrive).await;
    USB_DRIVE_READY.wait().await
}

pub(super) fn signal_ready(ready: bool) {
    READY.signal(ready);
}

pub(super) fn signal_shutdown_done() {
    SHUTDOWN_DONE.signal(());
}

pub(super) fn signal_usb_drive_ready(ready: bool) {
    USB_DRIVE_READY.signal(ready);
}

pub(super) async fn serve_filesystem<'a, D>(filesystem: &'a FatVolume<D>) -> FilesystemExit
where
    D: HadrisRead
        + HadrisWrite<Error = <D as HadrisRead>::Error>
        + HadrisSeek<Error = <D as HadrisRead>::Error>,
{
    let mut open_random_access: Option<OpenRandomAccessFile<'a, D>> = None;
    let mut last_handle = 0u32;

    loop {
        match COMMANDS.receive().await {
            Command::ListRoot => {
                let success = list_root(filesystem).await;
                ROOT_LIST_DONE.signal(success);
            }

            Command::ListDirectory(path) => {
                let result = list_directory(filesystem, &path).await;
                DIRECTORY_LIST_DONE.signal(result);
            }

            Command::OpenRandomAccess(path) => {
                // only one random-access file is required by the app today. Drop
                // the previous FAT reader before opening its replacement.
                open_random_access = None;

                let handle = next_handle(&mut last_handle);

                match OpenRandomAccessFile::open(filesystem, &path, handle).await {
                    Ok(file) => {
                        let handle = file.handle();

                        open_random_access = Some(file);

                        RANDOM_ACCESS_OPEN_DONE.signal(Ok(handle));
                    }

                    Err(error) => {
                        RANDOM_ACCESS_OPEN_DONE.signal(Err(error));
                    }
                }
            }

            Command::ReadRandomAccess {
                handle,
                offset,
                len,
            } => {
                let result = match open_random_access.as_mut() {
                    Some(file) if file.handle().id() == handle => {
                        file.read_exact_at(offset, len).await
                    }
                    _ => Err(StorageError::StaleHandle),
                };

                RANDOM_ACCESS_READ_DONE.signal(result);
            }

            Command::LoadState { name, max_bytes } => {
                let result = load_state(filesystem, &name, max_bytes).await;
                STATE_LOAD_DONE.signal(result);
            }

            Command::SaveState { name, bytes } => {
                let result = save_state(filesystem, &name, &bytes).await;
                STATE_SAVE_DONE.signal(result);
            }

            Command::EnterUsbDrive => {
                debug!("USB Drive storage handoff requested");

                // No FAT-backed object may survive the transition to raw block access.
                // This drops the currently-open EPUB reader before mount.rs drops
                // the FatVolume itself.
                drop(open_random_access);

                return FilesystemExit::UsbDrive;
            }

            Command::Shutdown => {
                debug!("storage shutdown requested");

                // ensure every FAT FileReader is gone before `mount.rs` drops
                // the FatVolume and block device.
                drop(open_random_access);

                return FilesystemExit::Shutdown;
            }
        }
    }
}

pub(super) async fn serve_unavailable_requests() {
    READY.signal(false);

    loop {
        match COMMANDS.receive().await {
            Command::ListRoot => ROOT_LIST_DONE.signal(false),
            Command::ListDirectory(_) => DIRECTORY_LIST_DONE.signal(Err(StorageError::Unavailable)),
            Command::OpenRandomAccess(_) => {
                RANDOM_ACCESS_OPEN_DONE.signal(Err(StorageError::Unavailable));
            }
            Command::ReadRandomAccess { .. } => {
                RANDOM_ACCESS_READ_DONE.signal(Err(StorageError::Unavailable));
            }
            Command::LoadState { .. } => STATE_LOAD_DONE.signal(Err(StorageError::Unavailable)),
            Command::SaveState { .. } => STATE_SAVE_DONE.signal(Err(StorageError::Unavailable)),
            Command::EnterUsbDrive => USB_DRIVE_READY.signal(false),
            Command::Shutdown => {
                SHUTDOWN_DONE.signal(());
                return;
            }
        }
    }
}

fn next_handle(last: &mut u32) -> u32 {
    *last = last.wrapping_add(1);

    if *last == 0 {
        *last = 1;
    }

    *last
}
