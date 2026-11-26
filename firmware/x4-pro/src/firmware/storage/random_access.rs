use defmt::warn;
use hadris_fat::r#async::{FatVolume, FatVolumeReadExt, FileEntry};
use hadris_io::{
    SeekFrom,
    r#async::{Read as HadrisRead, Seek as HadrisSeek},
};

use super::types::{RandomAccessHandle, StorageError};

pub(super) struct OpenRandomAccessFile {
    handle: RandomAccessHandle,
    entry: FileEntry,
}

impl OpenRandomAccessFile {
    pub(super) async fn open<D>(
        filesystem: &FatVolume<D>,
        path: &str,
        id: u32,
    ) -> Result<Self, StorageError>
    where
        D: HadrisRead + HadrisSeek<Error = <D as HadrisRead>::Error>,
    {
        let entry = filesystem.open_path(path).await.map_err(|error| {
            warn!(
                "random-access file open failed path={} error={:?}",
                path, error,
            );

            StorageError::Io
        })?;

        if entry.is_directory() {
            warn!("random-access open rejected directory path={}", path);
            return Err(StorageError::Io);
        }

        let handle = RandomAccessHandle::new(id, entry.len() as u64);

        Ok(Self { handle, entry })
    }

    pub(super) const fn handle(&self) -> RandomAccessHandle {
        self.handle
    }

    pub(super) async fn read_exact_at<D>(
        &self,
        filesystem: &FatVolume<D>,
        offset: u64,
        buffer: &mut [u8],
    ) -> Result<(), StorageError>
    where
        D: HadrisRead + HadrisSeek<Error = <D as HadrisRead>::Error>,
    {
        let len_u64 = u64::try_from(buffer.len()).map_err(|_| StorageError::OutOfBounds)?;
        let end = offset
            .checked_add(len_u64)
            .ok_or(StorageError::OutOfBounds)?;

        if end > self.handle.len() {
            return Err(StorageError::OutOfBounds);
        }

        if buffer.is_empty() {
            return Ok(());
        }

        let mut reader = filesystem.read_file(&self.entry).map_err(|error| {
            warn!("random-access reader open failed error={:?}", error);
            StorageError::Io
        })?;

        reader
            .seek(SeekFrom::Start(offset))
            .await
            .map_err(|error| {
                warn!("random-access seek failed error={:?}", error);
                StorageError::Io
            })?;

        let mut read = 0usize;
        while read < buffer.len() {
            let count = reader.read(&mut buffer[read..]).await.map_err(|error| {
                warn!("random-access read failed error={:?}", error);
                StorageError::Io
            })?;

            if count == 0 {
                return Err(StorageError::UnexpectedEof);
            }

            read += count;
        }

        Ok(())
    }
}
