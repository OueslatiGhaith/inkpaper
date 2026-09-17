use alloc::{vec, vec::Vec};

use defmt::warn;
use hadris_fat::r#async::FatVolume;
use hadris_io::{
    SeekFrom,
    r#async::{Read as HadrisRead, Seek as HadrisSeek},
};

use super::types::{MAX_RANDOM_ACCESS_READ_BYTES, RandomAccessHandle, StorageError};

pub(super) struct OpenRandomAccessFile<'a, D>
where
    D: HadrisRead + HadrisSeek<Error = <D as HadrisRead>::Error>,
{
    handle: RandomAccessHandle,
    reader: hadris_fat::r#async::read::FileReader<'a, D>,
}

impl<'a, D> OpenRandomAccessFile<'a, D>
where
    D: HadrisRead + HadrisSeek<Error = <D as HadrisRead>::Error>,
{
    pub(super) async fn open(
        filesystem: &'a FatVolume<D>,
        path: &str,
        id: u32,
    ) -> Result<Self, StorageError> {
        let reader = filesystem.open_file_path(path).await.map_err(|error| {
            warn!(
                "random-access file open failed path={} error={:?}",
                path, error,
            );

            StorageError::Io
        })?;

        let handle = RandomAccessHandle::new(id, reader.size() as u64);

        Ok(Self { handle, reader })
    }

    pub(super) const fn handle(&self) -> RandomAccessHandle {
        self.handle
    }

    pub(super) async fn read_exact_at(
        &mut self,
        offset: u64,
        len: usize,
    ) -> Result<Vec<u8>, StorageError> {
        if len > MAX_RANDOM_ACCESS_READ_BYTES {
            return Err(StorageError::ReadTooLarge);
        }

        let len_u64 = u64::try_from(len).map_err(|_| StorageError::OutOfBounds)?;

        let end = offset
            .checked_add(len_u64)
            .ok_or(StorageError::OutOfBounds)?;

        if end > self.handle.len() {
            return Err(StorageError::OutOfBounds);
        }

        self.reader
            .seek(SeekFrom::Start(offset))
            .await
            .map_err(|error| {
                warn!("random-access seek failed error={:?}", error);
                StorageError::Io
            })?;

        let mut bytes = vec![0u8; len];
        let mut read = 0usize;

        while read < bytes.len() {
            let count = self
                .reader
                .read(&mut bytes[read..])
                .await
                .map_err(|error| {
                    warn!("random-access read failed error={:?}", error);
                    StorageError::Io
                })?;

            if count == 0 {
                return Err(StorageError::UnexpectedEof);
            }

            read += count;
        }

        Ok(bytes)
    }
}
