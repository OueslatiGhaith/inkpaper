mod cache;
mod filesystem;
mod mount;
mod partition;
mod random_access;
mod service;
mod state;
mod types;

pub use mount::storage_task;

pub use service::{
    enter_usb_drive_and_wait, list_directory, load_state, open_random_access, read_file,
    read_random_access_into, save_state, shutdown_and_wait, wait_ready,
};

pub use types::{RandomAccessHandle, StorageEntry, StorageError};
