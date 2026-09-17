use alloc::vec::Vec;

use inkpaper_app::{AppPlatform, PlatformEntry};
use inkpaper_epub::EpubSource;

use crate::firmware::storage::{
    self, MAX_RANDOM_ACCESS_READ_BYTES, RandomAccessHandle, StorageError,
};

#[derive(Debug, Default, Clone, Copy)]
pub(super) struct X4Platform;

impl X4Platform {
    pub(super) const fn new() -> Self {
        Self
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct X4RandomAccessSource {
    handle: u32,
    len: u64,
}

impl X4RandomAccessSource {
    const fn from_handle(handle: RandomAccessHandle) -> Self {
        Self {
            handle: handle.id(),
            len: handle.len(),
        }
    }
}

impl EpubSource for X4RandomAccessSource {
    type Error = StorageError;

    async fn len(&mut self) -> Result<u64, Self::Error> {
        Ok(self.len)
    }

    async fn read_exact_at(&mut self, offset: u64, buffer: &mut [u8]) -> Result<(), Self::Error> {
        let requested_len = u64::try_from(buffer.len()).map_err(|_| StorageError::OutOfBounds)?;

        let end = offset
            .checked_add(requested_len)
            .ok_or(StorageError::OutOfBounds)?;

        if end > self.len {
            return Err(StorageError::OutOfBounds);
        }

        let mut copied = 0usize;

        while copied < buffer.len() {
            let remaining = buffer.len() - copied;

            let chunk_len = remaining.min(MAX_RANDOM_ACCESS_READ_BYTES);

            let chunk_offset = offset
                .checked_add(copied as u64)
                .ok_or(StorageError::OutOfBounds)?;

            let chunk =
                storage::read_random_access_and_wait(self.handle, chunk_offset, chunk_len).await?;

            if chunk.len() != chunk_len {
                return Err(StorageError::UnexpectedEof);
            }

            buffer[copied..copied + chunk_len].copy_from_slice(&chunk);

            copied += chunk_len;
        }

        Ok(())
    }
}

impl AppPlatform for X4Platform {
    type Error = StorageError;
    type RandomAccessSource = X4RandomAccessSource;

    async fn list_directory(&mut self, path: &str) -> Result<Vec<PlatformEntry>, Self::Error> {
        let entries = storage::list_directory_and_wait(path).await?;

        Ok(entries
            .into_iter()
            .map(|entry| {
                let (name, is_directory) = entry.into_parts();

                if is_directory {
                    PlatformEntry::directory(name)
                } else {
                    PlatformEntry::file(name)
                }
            })
            .collect())
    }

    async fn open_random_access(
        &mut self,
        path: &str,
    ) -> Result<Self::RandomAccessSource, Self::Error> {
        let handle = storage::open_random_access_and_wait(path).await?;

        Ok(X4RandomAccessSource::from_handle(handle))
    }

    async fn load_state(
        &mut self,
        name: &str,
        max_bytes: usize,
    ) -> Result<Option<Vec<u8>>, Self::Error> {
        storage::load_state_and_wait(name, max_bytes).await
    }

    async fn save_state(&mut self, name: &str, bytes: &[u8]) -> Result<(), Self::Error> {
        storage::save_state_and_wait(name, bytes).await
    }
}
