mod filesystem;
mod mount;
mod partition;
mod random_access;
mod service;
mod state;
mod types;

pub use mount::storage_task;

pub use service::{
    list_directory_and_wait, list_root_and_wait, load_state_and_wait, open_random_access_and_wait,
    read_random_access_and_wait, save_state_and_wait, shutdown_and_wait, wait_ready,
};

pub use types::{MAX_RANDOM_ACCESS_READ_BYTES, RandomAccessHandle, StorageEntry, StorageError};
