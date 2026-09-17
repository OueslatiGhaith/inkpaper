use alloc::string::String;

pub const MAX_RANDOM_ACCESS_READ_BYTES: usize = 16 * 1024;
pub(super) const MAX_STATE_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageError {
    Unavailable,
    Io,
    InvalidStateName,
    StateTooLarge,
    StaleHandle,
    ReadTooLarge,
    OutOfBounds,
    UnexpectedEof,
}

#[derive(Debug)]
pub struct StorageEntry {
    name: String,
    is_directory: bool,
}

impl StorageEntry {
    pub(super) fn new(name: String, is_directory: bool) -> Self {
        Self { name, is_directory }
    }

    pub fn into_parts(self) -> (String, bool) {
        (self.name, self.is_directory)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RandomAccessHandle {
    id: u32,
    len: u64,
}

impl RandomAccessHandle {
    pub(super) const fn new(id: u32, len: u64) -> Self {
        Self { id, len }
    }

    pub const fn id(self) -> u32 {
        self.id
    }

    pub const fn len(self) -> u64 {
        self.len
    }
}
