use alloc::{string::String, vec::Vec};

use defmt::debug;
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel, signal::Signal,
};
use hadris_fat::r#async::FatVolume;
use hadris_io::r#async::{Read as HadrisRead, Seek as HadrisSeek, Write as HadrisWrite};

use super::{
    filesystem::{list_directory, read_file},
    random_access::OpenRandomAccessFile,
    state::{load_state, save_state},
    types::{
        MAX_RANDOM_ACCESS_READ_BYTES, MAX_STATE_BYTES, RandomAccessHandle, StorageEntry,
        StorageError,
    },
};
use crate::firmware::rpc::{self, Reply};

const COMMAND_CAPACITY: usize = 4;

static COMMANDS: Channel<CriticalSectionRawMutex, Command, COMMAND_CAPACITY> = Channel::new();

static READY: Signal<CriticalSectionRawMutex, bool> = Signal::new();

/// Why the storage task stopped serving the filesystem, with the reply that
/// acknowledges the transition once `mount.rs` has made it.
#[derive(Debug)]
pub(super) enum FilesystemExit {
    Shutdown(Reply<()>),
    UsbDrive(Reply<bool>),
}

#[derive(Debug)]
enum Command {
    ListDirectory {
        path: String,
        reply: Reply<Result<Vec<StorageEntry>, StorageError>>,
    },

    OpenRandomAccess {
        path: String,
        reply: Reply<Result<RandomAccessHandle, StorageError>>,
    },

    ReadRandomAccess {
        handle: u32,
        offset: u64,
        len: usize,
        reply: Reply<Result<Vec<u8>, StorageError>>,
    },

    ReadFile {
        path: String,
        max_bytes: usize,
        reply: Reply<Result<Vec<u8>, StorageError>>,
    },

    LoadState {
        name: String,
        max_bytes: usize,
        reply: Reply<Result<Option<Vec<u8>>, StorageError>>,
    },

    SaveState {
        name: String,
        bytes: Vec<u8>,
        reply: Reply<Result<(), StorageError>>,
    },

    EnterUsbDrive(Reply<bool>),

    Shutdown(Reply<()>),
}

pub async fn wait_ready() -> bool {
    READY.wait().await
}

pub async fn list_directory_and_wait(path: &str) -> Result<Vec<StorageEntry>, StorageError> {
    rpc::call(&COMMANDS, |reply| Command::ListDirectory {
        path: String::from(path),
        reply,
    })
    .await
}

pub async fn open_random_access_and_wait(path: &str) -> Result<RandomAccessHandle, StorageError> {
    rpc::call(&COMMANDS, |reply| Command::OpenRandomAccess {
        path: String::from(path),
        reply,
    })
    .await
}

pub async fn read_random_access_and_wait(
    handle: u32,
    offset: u64,
    len: usize,
) -> Result<Vec<u8>, StorageError> {
    if len > MAX_RANDOM_ACCESS_READ_BYTES {
        return Err(StorageError::ReadTooLarge);
    }

    rpc::call(&COMMANDS, |reply| Command::ReadRandomAccess {
        handle,
        offset,
        len,
        reply,
    })
    .await
}

/// Reads a whole file, apart from the open random-access file.
pub async fn read_file_and_wait(path: &str, max_bytes: usize) -> Result<Vec<u8>, StorageError> {
    rpc::call(&COMMANDS, |reply| Command::ReadFile {
        path: String::from(path),
        max_bytes,
        reply,
    })
    .await
}

pub async fn load_state_and_wait(
    name: &str,
    max_bytes: usize,
) -> Result<Option<Vec<u8>>, StorageError> {
    if max_bytes > MAX_STATE_BYTES {
        return Err(StorageError::StateTooLarge);
    }

    rpc::call(&COMMANDS, |reply| Command::LoadState {
        name: String::from(name),
        max_bytes,
        reply,
    })
    .await
}

pub async fn save_state_and_wait(name: &str, bytes: &[u8]) -> Result<(), StorageError> {
    if bytes.len() > MAX_STATE_BYTES {
        return Err(StorageError::StateTooLarge);
    }

    rpc::call(&COMMANDS, |reply| Command::SaveState {
        name: String::from(name),
        bytes: Vec::from(bytes),
        reply,
    })
    .await
}

pub async fn shutdown_and_wait() {
    rpc::call(&COMMANDS, Command::Shutdown).await;
}

pub async fn enter_usb_drive_and_wait() -> bool {
    rpc::call(&COMMANDS, Command::EnterUsbDrive).await
}

pub(super) fn signal_ready(ready: bool) {
    READY.signal(ready);
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
            Command::ListDirectory { path, reply } => {
                reply.send(list_directory(filesystem, &path).await);
            }

            Command::OpenRandomAccess { path, reply } => {
                // only one random-access file is required by the app today. Drop
                // the previous FAT reader before opening its replacement.
                open_random_access = None;

                let handle = next_handle(&mut last_handle);

                match OpenRandomAccessFile::open(filesystem, &path, handle).await {
                    Ok(file) => {
                        let handle = file.handle();

                        open_random_access = Some(file);

                        reply.send(Ok(handle));
                    }

                    Err(error) => reply.send(Err(error)),
                }
            }

            Command::ReadRandomAccess {
                handle,
                offset,
                len,
                reply,
            } => {
                let result = match open_random_access.as_mut() {
                    Some(file) if file.handle().id() == handle => {
                        file.read_exact_at(offset, len).await
                    }
                    _ => Err(StorageError::StaleHandle),
                };

                reply.send(result);
            }

            Command::ReadFile {
                path,
                max_bytes,
                reply,
            } => {
                reply.send(read_file(filesystem, &path, max_bytes).await);
            }

            Command::LoadState {
                name,
                max_bytes,
                reply,
            } => {
                reply.send(load_state(filesystem, &name, max_bytes).await);
            }

            Command::SaveState { name, bytes, reply } => {
                reply.send(save_state(filesystem, &name, &bytes).await);
            }

            Command::EnterUsbDrive(reply) => {
                debug!("USB Drive storage handoff requested");

                // No FAT-backed object may survive the transition to raw block access.
                // This drops the currently-open EPUB reader before mount.rs drops
                // the FatVolume itself.
                drop(open_random_access);

                return FilesystemExit::UsbDrive(reply);
            }

            Command::Shutdown(reply) => {
                debug!("storage shutdown requested");

                // ensure every FAT FileReader is gone before `mount.rs` drops
                // the FatVolume and block device.
                drop(open_random_access);

                return FilesystemExit::Shutdown(reply);
            }
        }
    }
}

pub(super) async fn serve_unavailable_requests() {
    READY.signal(false);

    refuse_filesystem_requests(false).await.send(());
}

/// Refuses filesystem requests while the card is a USB drive, until a shutdown
/// request. The drive's exit acknowledges it once it has stopped.
pub(super) async fn serve_usb_drive_requests() -> Reply<()> {
    refuse_filesystem_requests(true).await
}

/// Answers every filesystem request with [`StorageError::Unavailable`] and a
/// USB drive request with `usb_drive`, until a shutdown request.
async fn refuse_filesystem_requests(usb_drive: bool) -> Reply<()> {
    loop {
        match COMMANDS.receive().await {
            Command::ListDirectory { reply, .. } => reply.send(Err(StorageError::Unavailable)),
            Command::OpenRandomAccess { reply, .. } => reply.send(Err(StorageError::Unavailable)),
            Command::ReadRandomAccess { reply, .. } => reply.send(Err(StorageError::Unavailable)),
            Command::ReadFile { reply, .. } => reply.send(Err(StorageError::Unavailable)),
            Command::LoadState { reply, .. } => reply.send(Err(StorageError::Unavailable)),
            Command::SaveState { reply, .. } => reply.send(Err(StorageError::Unavailable)),
            Command::EnterUsbDrive(reply) => reply.send(usb_drive),
            Command::Shutdown(reply) => return reply,
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
