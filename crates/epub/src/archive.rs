use alloc::{vec, vec::Vec};
use miniz_oxide::inflate::decompress_to_vec_with_limit;

use crate::{
    error::{ArchiveError, Error},
    path::ArchivePath,
    source::EpubSource,
};

const EOCD_SIGNATURE: u32 = 0x0605_4b50;

const CENTRAL_HEADER_SIGNATURE: u32 = 0x0201_4b50;

const LOCAL_HEADER_SIGNATURE: u32 = 0x0403_4b50;

const EOCD_MIN_LEN: usize = 22;
const CENTRAL_HEADER_LEN: usize = 46;
const LOCAL_HEADER_LEN: usize = 30;

const MAX_EOCD_SEARCH: usize = EOCD_MIN_LEN + u16::MAX as usize;

const COMPRESSION_STORED: u16 = 0;
const COMPRESSION_DEFLATE: u16 = 8;

struct Entry {
    flags: u16,
    compression: u16,
    compressed_size: u32,
    uncompressed_size: u32,
    local_header_offset: u32,
}

pub(crate) struct Archive<S> {
    source: S,
    archive_len: u64,
    central_directory_offset: u64,
    central_directory_size: u64,
    entry_count: u16,
}

impl<S> Archive<S>
where
    S: EpubSource,
{
    pub(crate) async fn open(mut source: S) -> Result<Self, Error<S::Error>> {
        let archive_len = source.len().await.map_err(Error::Source)?;
        if archive_len < EOCD_MIN_LEN as u64 {
            return Err(Error::Archive(ArchiveError::EndOfCentralDirectoryMissing));
        }

        let tail_len = archive_len.min(MAX_EOCD_SEARCH as u64);
        let tail_len =
            usize::try_from(tail_len).map_err(|_| Error::Archive(ArchiveError::SizeOverflow))?;

        let tail_offset = archive_len
            .checked_sub(tail_len as u64)
            .ok_or(Error::Archive(ArchiveError::SizeOverflow))?;

        let mut tail = vec![0; tail_len];
        source
            .read_exact_at(tail_offset, &mut tail)
            .await
            .map_err(Error::Source)?;

        let eocd =
            find_eocd(&tail).ok_or(Error::Archive(ArchiveError::EndOfCentralDirectoryMissing))?;

        let disk = le_u16(&tail[eocd + 4..eocd + 6]);
        let central_disk = le_u16(&tail[eocd + 6..eocd + 8]);
        let disk_entries = le_u16(&tail[eocd + 8..eocd + 10]);
        let entry_count = le_u16(&tail[eocd + 10..eocd + 12]);

        if disk != 0 || central_disk != 0 || disk_entries != entry_count {
            return Err(Error::Archive(ArchiveError::MultiDiskUnsupported));
        }

        let central_directory_size = le_u32(&tail[eocd + 12..eocd + 16]);
        let central_directory_offset = le_u32(&tail[eocd + 16..eocd + 20]);

        if entry_count == u16::MAX
            || central_directory_size == u32::MAX
            || central_directory_offset == u32::MAX
        {
            return Err(Error::Archive(ArchiveError::Zip64Unsupported));
        }

        let central_directory_end = u64::from(central_directory_offset)
            .checked_add(u64::from(central_directory_size))
            .ok_or(Error::Archive(ArchiveError::SizeOverflow))?;

        if central_directory_end > archive_len {
            return Err(Error::Archive(ArchiveError::InvalidCentralDirectory));
        }

        Ok(Self {
            source,
            archive_len,
            central_directory_offset: u64::from(central_directory_offset),
            central_directory_size: u64::from(central_directory_size),
            entry_count,
        })
    }

    pub(crate) async fn read_entry(
        &mut self,
        path: &ArchivePath,
    ) -> Result<Vec<u8>, Error<S::Error>> {
        let entry = self.find_entry(path).await?;
        if entry.flags & 0x0001 != 0 {
            return Err(Error::Archive(ArchiveError::EncryptedEntry));
        }

        let mut local_header = [0u8; LOCAL_HEADER_LEN];
        self.read_exact(u64::from(entry.local_header_offset), &mut local_header)
            .await?;

        if le_u32(&local_header[0..4]) != LOCAL_HEADER_SIGNATURE {
            return Err(Error::Archive(ArchiveError::InvalidLocalHeader));
        }

        let local_flags = le_u16(&local_header[6..8]);
        let local_compression = le_u16(&local_header[8..10]);

        if local_flags & 0x0001 != 0 {
            return Err(Error::Archive(ArchiveError::EncryptedEntry));
        }
        if local_compression != entry.compression {
            return Err(Error::Archive(ArchiveError::InvalidLocalHeader));
        }

        let name_len = le_u16(&local_header[26..28]);
        let extra_len = le_u16(&local_header[28..30]);

        let data_offset = u64::from(entry.local_header_offset)
            .checked_add(LOCAL_HEADER_LEN as u64)
            .and_then(|offset| offset.checked_add(u64::from(name_len)))
            .and_then(|offset| offset.checked_add(u64::from(extra_len)))
            .ok_or(Error::Archive(ArchiveError::SizeOverflow))?;

        let data_end = data_offset
            .checked_add(u64::from(entry.compressed_size))
            .ok_or(Error::Archive(ArchiveError::SizeOverflow))?;

        if data_end > self.archive_len {
            return Err(Error::Archive(ArchiveError::InvalidLocalHeader));
        }

        let compressed_len = usize::try_from(entry.compressed_size)
            .map_err(|_| Error::Archive(ArchiveError::SizeOverflow))?;

        let expected_len = usize::try_from(entry.uncompressed_size)
            .map_err(|_| Error::Archive(ArchiveError::SizeOverflow))?;

        let mut compressed = vec![0u8; compressed_len];

        self.read_exact(data_offset, &mut compressed).await?;

        match entry.compression {
            COMPRESSION_STORED => {
                if compressed.len() != expected_len {
                    return Err(Error::Archive(ArchiveError::SizeMismatch {
                        expected: entry.uncompressed_size,
                        actual: compressed.len(),
                    }));
                }

                Ok(compressed)
            }
            COMPRESSION_DEFLATE => {
                let decompressed = decompress_to_vec_with_limit(&compressed, expected_len)
                    .map_err(|_| Error::Archive(ArchiveError::Inflate))?;

                if decompressed.len() != expected_len {
                    return Err(Error::Archive(ArchiveError::SizeMismatch {
                        expected: entry.uncompressed_size,
                        actual: decompressed.len(),
                    }));
                }

                Ok(decompressed)
            }
            compression => Err(Error::Archive(ArchiveError::UnsupportedCompression(
                compression,
            ))),
        }
    }

    pub(crate) fn into_source(self) -> S {
        self.source
    }

    async fn find_entry(&mut self, path: &ArchivePath) -> Result<Entry, Error<S::Error>> {
        let central_end = self.central_directory_offset + self.central_directory_size;

        let mut offset = self.central_directory_offset;

        for _ in 0..self.entry_count {
            if offset
                .checked_add(CENTRAL_HEADER_LEN as u64)
                .is_none_or(|end| end > central_end)
            {
                return Err(Error::Archive(ArchiveError::InvalidCentralDirectory));
            }

            let mut header = [0u8; CENTRAL_HEADER_LEN];
            self.read_exact(offset, &mut header).await?;

            if le_u32(&header[0..4]) != CENTRAL_HEADER_SIGNATURE {
                return Err(Error::Archive(ArchiveError::InvalidCentralDirectory));
            }

            let flags = le_u16(&header[8..10]);
            let compression = le_u16(&header[10..12]);
            let compressed_size = le_u32(&header[20..24]);
            let uncompressed_size = le_u32(&header[24..28]);
            let name_len = le_u16(&header[28..30]);
            let extra_len = le_u16(&header[30..32]);
            let comment_len = le_u16(&header[32..34]);
            let disk_start = le_u16(&header[34..36]);
            let local_header_offset = le_u32(&header[42..46]);
            let variable_len = u64::from(name_len) + u64::from(extra_len) + u64::from(comment_len);

            let next_offset = offset
                .checked_add(CENTRAL_HEADER_LEN as u64)
                .and_then(|value| value.checked_add(variable_len))
                .ok_or(Error::Archive(ArchiveError::SizeOverflow))?;

            if next_offset > central_end {
                return Err(Error::Archive(ArchiveError::InvalidCentralDirectory));
            }

            let mut name = vec![0u8; usize::from(name_len)];

            self.read_exact(offset + CENTRAL_HEADER_LEN as u64, &mut name)
                .await?;

            let name = core::str::from_utf8(&name).map_err(Error::Utf8)?;
            if name == path.as_str() {
                if disk_start != 0 {
                    return Err(Error::Archive(ArchiveError::MultiDiskUnsupported));
                }

                if compressed_size == u32::MAX
                    || uncompressed_size == u32::MAX
                    || local_header_offset == u32::MAX
                {
                    return Err(Error::Archive(ArchiveError::Zip64Unsupported));
                }

                return Ok(Entry {
                    flags,
                    compression,
                    compressed_size,
                    uncompressed_size,
                    local_header_offset,
                });
            }

            offset = next_offset;
        }

        Err(Error::Archive(ArchiveError::EntryNotFound))
    }

    async fn read_exact(&mut self, offset: u64, buffer: &mut [u8]) -> Result<(), Error<S::Error>> {
        self.source
            .read_exact_at(offset, buffer)
            .await
            .map_err(Error::Source)
    }
}

fn find_eocd(tail: &[u8]) -> Option<usize> {
    if tail.len() < EOCD_MIN_LEN {
        return None;
    }

    for index in (0..=tail.len() - EOCD_MIN_LEN).rev() {
        if le_u32(&tail[index..index + 4]) != EOCD_SIGNATURE {
            continue;
        }

        let comment_len = usize::from(le_u16(&tail[index + 20..index + 22]));

        if index + EOCD_MIN_LEN + comment_len == tail.len() {
            return Some(index);
        }
    }

    None
}

fn le_u16(bytes: &[u8]) -> u16 {
    u16::from_le_bytes([bytes[0], bytes[1]])
}

fn le_u32(bytes: &[u8]) -> u32 {
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}
