use alloc::{boxed::Box, string::String, vec, vec::Vec};
use inkpaper_io::ByteSource;
use miniz_oxide::inflate::{
    TINFLStatus,
    core::{DecompressorOxide, decompress, inflate_flags::TINFL_FLAG_HAS_MORE_INPUT},
    decompress_to_vec_with_limit,
};

use crate::{
    error::{ArchiveError, Error},
    path::ArchivePath,
};

const EOCD_SIGNATURE: u32 = 0x0605_4b50;

const CENTRAL_HEADER_SIGNATURE: u32 = 0x0201_4b50;

const LOCAL_HEADER_SIGNATURE: u32 = 0x0403_4b50;

const EOCD_MIN_LEN: usize = 22;
const CENTRAL_HEADER_LEN: usize = 46;
const LOCAL_HEADER_LEN: usize = 30;

const FAST_EOCD_PROBE: usize = 512;
const MAX_EOCD_SEARCH: usize = EOCD_MIN_LEN + u16::MAX as usize;

const COMPRESSION_STORED: u16 = 0;
const COMPRESSION_DEFLATE: u16 = 8;

const STREAM_STORED_BUF_LEN: usize = 128;
const STREAM_DEFLATE_IN_LEN: usize = 1024;
const STREAM_DEFLATE_OUT_LEN: usize = 32 * 1024;

#[derive(Debug, Clone, Copy)]
struct EntryData {
    compression: u16,
    data_offset: u64,
    compressed_len: usize,
    uncompressed_len: usize,
    uncompressed_size: u32,
}

#[derive(Debug, Clone, Copy)]
struct Entry {
    flags: u16,
    compression: u16,
    compressed_size: u32,
    uncompressed_size: u32,
    local_header_offset: u32,
}

#[derive(Debug, Clone, Copy)]
struct IndexedEntry {
    entry: Entry,
    disk_start: u16,
}

impl IndexedEntry {
    fn validated_entry(self) -> Result<Entry, ArchiveError> {
        if self.disk_start != 0 {
            return Err(ArchiveError::MultiDiskUnsupported);
        }

        if self.entry.compressed_size == u32::MAX
            || self.entry.uncompressed_size == u32::MAX
            || self.entry.local_header_offset == u32::MAX
        {
            return Err(ArchiveError::Zip64Unsupported);
        }

        Ok(self.entry)
    }
}

pub(crate) struct Archive<S> {
    source: S,
    archive_len: u64,
    entries: Vec<(String, IndexedEntry)>,
}

impl<S> Archive<S>
where
    S: ByteSource,
{
    pub(crate) async fn open(mut source: S) -> Result<Self, Error<S::Error>> {
        let archive_len = source.len().await.map_err(Error::Source)?;
        if archive_len < EOCD_MIN_LEN as u64 {
            return Err(Error::Archive(ArchiveError::EndOfCentralDirectoryMissing));
        }

        let eocd = read_eocd(&mut source, archive_len).await?;

        let disk = le_u16(&eocd[4..6]);
        let central_disk = le_u16(&eocd[6..8]);
        let disk_entries = le_u16(&eocd[8..10]);
        let entry_count = le_u16(&eocd[10..12]);

        if disk != 0 || central_disk != 0 || disk_entries != entry_count {
            return Err(Error::Archive(ArchiveError::MultiDiskUnsupported));
        }

        let central_directory_size = le_u32(&eocd[12..16]);
        let central_directory_offset = le_u32(&eocd[16..20]);

        if entry_count == u16::MAX
            || central_directory_size == u32::MAX
            || central_directory_offset == u32::MAX
        {
            return Err(Error::Archive(ArchiveError::Zip64Unsupported));
        }

        let central_directory_offset = u64::from(central_directory_offset);
        let central_directory_size_u64 = u64::from(central_directory_size);

        let central_directory_end = central_directory_offset
            .checked_add(central_directory_size_u64)
            .ok_or(Error::Archive(ArchiveError::SizeOverflow))?;

        if central_directory_end > archive_len {
            return Err(Error::Archive(ArchiveError::InvalidCentralDirectory));
        }

        let central_directory_len = usize::try_from(central_directory_size)
            .map_err(|_| Error::Archive(ArchiveError::SizeOverflow))?;

        let mut central_directory = vec![0u8; central_directory_len];
        source
            .read_exact_at(central_directory_offset, &mut central_directory)
            .await
            .map_err(Error::Source)?;

        let entries = parse_central_directory(&central_directory, entry_count)?;

        Ok(Self {
            source,
            archive_len,
            entries,
        })
    }

    pub(crate) async fn read_entry(
        &mut self,
        path: &ArchivePath,
    ) -> Result<Vec<u8>, Error<S::Error>> {
        let data = self.resolve_entry_data(path).await?;
        let mut compressed = vec![0u8; data.compressed_len];

        self.read_exact(data.data_offset, &mut compressed).await?;

        match data.compression {
            COMPRESSION_STORED => {
                if compressed.len() != data.uncompressed_len {
                    return Err(Error::Archive(ArchiveError::SizeMismatch {
                        expected: data.uncompressed_size,
                        actual: compressed.len(),
                    }));
                }

                Ok(compressed)
            }
            COMPRESSION_DEFLATE => {
                let decompressed = decompress_to_vec_with_limit(&compressed, data.uncompressed_len)
                    .map_err(|_| Error::Archive(ArchiveError::Inflate))?;

                if decompressed.len() != data.uncompressed_len {
                    return Err(Error::Archive(ArchiveError::SizeMismatch {
                        expected: data.uncompressed_size,
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

    pub(crate) async fn open_entry_stream<'a>(
        &'a mut self,
        path: &ArchivePath,
    ) -> Result<EntryStream<'a, S>, Error<S::Error>> {
        let data = self.resolve_entry_data(path).await?;

        match data.compression {
            COMPRESSION_STORED => {
                if data.compressed_len != data.uncompressed_len {
                    return Err(Error::Archive(ArchiveError::SizeMismatch {
                        expected: data.uncompressed_size,
                        actual: data.compressed_len,
                    }));
                }

                Ok(EntryStream::Stored(StoredStream {
                    source: &mut self.source,
                    offset: data.data_offset,
                    remaining: data.uncompressed_len,
                    buf: [0u8; STREAM_STORED_BUF_LEN],
                    buf_pos: 0,
                    buf_len: 0,
                }))
            }
            COMPRESSION_DEFLATE => Ok(EntryStream::Deflate(Box::new(DeflateStream {
                source: &mut self.source,
                compressed_offset: data.data_offset,
                compressed_remaining: data.compressed_len,
                uncompressed_remaining: data.uncompressed_len,
                decompressor: Box::default(),
                in_buf: vec![0u8; STREAM_DEFLATE_IN_LEN],
                in_pos: 0,
                in_len: 0,
                out_buf: vec![0u8; STREAM_DEFLATE_OUT_LEN],
                out_read: 0,
                out_end: 0,
                finished: false,
            }))),
            compression => Err(Error::Archive(ArchiveError::UnsupportedCompression(
                compression,
            ))),
        }
    }

    async fn resolve_entry_data(
        &mut self,
        path: &ArchivePath,
    ) -> Result<EntryData, Error<S::Error>> {
        let entry = self.find_entry(path).map_err(Error::Archive)?;
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

        let uncompressed_len = usize::try_from(entry.uncompressed_size)
            .map_err(|_| Error::Archive(ArchiveError::SizeOverflow))?;

        Ok(EntryData {
            compression: entry.compression,
            data_offset,
            compressed_len,
            uncompressed_len,
            uncompressed_size: entry.uncompressed_size,
        })
    }

    pub(crate) fn uncompressed_sizes(
        &self,
        paths: &[ArchivePath],
    ) -> Result<Vec<Option<u64>>, ArchiveError> {
        let mut sizes = Vec::with_capacity(paths.len());

        for path in paths {
            match self.lookup_indexed(path) {
                Some(indexed) => {
                    let entry = indexed.validated_entry()?;
                    sizes.push(Some(u64::from(entry.uncompressed_size)));
                }
                None => sizes.push(None),
            }
        }

        Ok(sizes)
    }

    fn lookup_indexed(&self, path: &ArchivePath) -> Option<IndexedEntry> {
        let index = self
            .entries
            .binary_search_by(|(name, _)| name.as_str().cmp(path.as_str()))
            .ok()?;

        Some(self.entries[index].1)
    }

    fn find_entry(&self, path: &ArchivePath) -> Result<Entry, ArchiveError> {
        let indexed = self
            .lookup_indexed(path)
            .ok_or(ArchiveError::EntryNotFound)?;

        indexed.validated_entry()
    }

    async fn read_exact(&mut self, offset: u64, buffer: &mut [u8]) -> Result<(), Error<S::Error>> {
        self.source
            .read_exact_at(offset, buffer)
            .await
            .map_err(Error::Source)
    }
}

pub(crate) enum EntryStream<'a, S> {
    Stored(StoredStream<'a, S>),
    Deflate(Box<DeflateStream<'a, S>>),
}

pub(crate) struct StoredStream<'a, S> {
    source: &'a mut S,
    offset: u64,
    remaining: usize,
    buf: [u8; STREAM_STORED_BUF_LEN],
    buf_pos: usize,
    buf_len: usize,
}

pub(crate) struct DeflateStream<'a, S> {
    source: &'a mut S,
    compressed_offset: u64,
    compressed_remaining: usize,
    uncompressed_remaining: usize,
    decompressor: Box<DecompressorOxide>,
    in_buf: Vec<u8>,
    in_pos: usize,
    in_len: usize,
    out_buf: Vec<u8>,
    out_read: usize,
    out_end: usize,
    finished: bool,
}

impl<S: ByteSource> EntryStream<'_, S> {
    pub(crate) async fn read_u8(&mut self) -> Result<Option<u8>, Error<S::Error>> {
        let mut byte = [0u8; 1];
        if self.read_exact(&mut byte).await? {
            Ok(Some(byte[0]))
        } else {
            Ok(None)
        }
    }

    /// Reads `dst.len()` bytes into `dst`, returning `Ok(false)` if EOF is reached first.
    pub(crate) async fn read_exact(&mut self, dst: &mut [u8]) -> Result<bool, Error<S::Error>> {
        match self {
            Self::Stored(stream) => stream.read_exact(dst).await,
            Self::Deflate(stream) => stream.read_exact(dst).await,
        }
    }

    /// Advances past `count` bytes, returning `Ok(false)` if EOF is reached first.
    pub(crate) async fn skip(&mut self, count: usize) -> Result<bool, Error<S::Error>> {
        match self {
            Self::Stored(stream) => Ok(stream.skip(count)),
            Self::Deflate(stream) => stream.skip(count).await,
        }
    }
}

impl<S: ByteSource> StoredStream<'_, S> {
    async fn read_exact(&mut self, mut dst: &mut [u8]) -> Result<bool, Error<S::Error>> {
        let buffered = self.buf_len - self.buf_pos;
        if dst.len() > buffered.saturating_add(self.remaining) {
            return Ok(false);
        }

        while !dst.is_empty() {
            let avail = self.buf_len - self.buf_pos;
            if avail > 0 {
                let take = avail.min(dst.len());
                dst[..take].copy_from_slice(&self.buf[self.buf_pos..self.buf_pos + take]);
                self.buf_pos += take;
                dst = &mut dst[take..];
                continue;
            }

            if dst.len() >= self.buf.len() {
                let read_len = dst.len();
                self.source
                    .read_exact_at(self.offset, dst)
                    .await
                    .map_err(Error::Source)?;
                self.offset += read_len as u64;
                self.remaining -= read_len;
                break;
            }

            let fill = self.remaining.min(self.buf.len());
            self.source
                .read_exact_at(self.offset, &mut self.buf[..fill])
                .await
                .map_err(Error::Source)?;
            self.offset += fill as u64;
            self.remaining -= fill;
            self.buf_pos = 0;
            self.buf_len = fill;
        }

        Ok(true)
    }

    fn skip(&mut self, count: usize) -> bool {
        let avail = self.buf_len - self.buf_pos;
        if count <= avail {
            self.buf_pos += count;
            return true;
        }

        let extra = count - avail;
        if extra > self.remaining {
            return false;
        }

        self.buf_pos = 0;
        self.buf_len = 0;
        self.offset += extra as u64;
        self.remaining -= extra;
        true
    }
}

impl<S: ByteSource> DeflateStream<'_, S> {
    async fn fill_output(&mut self) -> Result<usize, Error<S::Error>> {
        while self.out_read == self.out_end {
            if self.finished || self.uncompressed_remaining == 0 {
                return Ok(0);
            }

            if self.out_end == self.out_buf.len() {
                self.out_read = 0;
                self.out_end = 0;
            }

            if self.in_pos == self.in_len && self.compressed_remaining > 0 {
                let to_read = self.compressed_remaining.min(self.in_buf.len());
                self.source
                    .read_exact_at(self.compressed_offset, &mut self.in_buf[..to_read])
                    .await
                    .map_err(Error::Source)?;
                self.compressed_offset += to_read as u64;
                self.compressed_remaining -= to_read;
                self.in_pos = 0;
                self.in_len = to_read;
            }

            let flags = if self.compressed_remaining > 0 {
                TINFL_FLAG_HAS_MORE_INPUT
            } else {
                0
            };

            let (status, consumed, written) = decompress(
                &mut self.decompressor,
                &self.in_buf[self.in_pos..self.in_len],
                &mut self.out_buf,
                self.out_end,
                flags,
            );

            self.in_pos += consumed;

            if written > self.uncompressed_remaining {
                return Err(Error::Archive(ArchiveError::Inflate));
            }

            self.out_end += written;
            self.uncompressed_remaining -= written;

            match status {
                TINFLStatus::Done => {
                    self.finished = true;
                }
                TINFLStatus::HasMoreOutput | TINFLStatus::NeedsMoreInput => {
                    if written == 0 && consumed == 0 && self.compressed_remaining == 0 {
                        return Err(Error::Archive(ArchiveError::Inflate));
                    }
                }
                _ => return Err(Error::Archive(ArchiveError::Inflate)),
            }
        }

        Ok(self.out_end - self.out_read)
    }

    async fn read_exact(&mut self, mut dst: &mut [u8]) -> Result<bool, Error<S::Error>> {
        while !dst.is_empty() {
            let avail = self.fill_output().await?;
            if avail == 0 {
                return Ok(false);
            }

            let take = avail.min(dst.len());
            dst[..take].copy_from_slice(&self.out_buf[self.out_read..self.out_read + take]);
            self.out_read += take;
            dst = &mut dst[take..];
        }

        Ok(true)
    }

    async fn skip(&mut self, mut count: usize) -> Result<bool, Error<S::Error>> {
        while count > 0 {
            let avail = self.fill_output().await?;
            if avail == 0 {
                return Ok(false);
            }

            let take = avail.min(count);
            self.out_read += take;
            count -= take;
        }

        Ok(true)
    }
}

async fn read_eocd<S: ByteSource>(
    source: &mut S,
    archive_len: u64,
) -> Result<[u8; EOCD_MIN_LEN], Error<S::Error>> {
    let fast_len = archive_len.min(FAST_EOCD_PROBE as u64) as usize;
    let fast_offset = archive_len
        .checked_sub(fast_len as u64)
        .ok_or(Error::Archive(ArchiveError::SizeOverflow))?;

    let mut fast_buf = [0u8; FAST_EOCD_PROBE];
    source
        .read_exact_at(fast_offset, &mut fast_buf[..fast_len])
        .await
        .map_err(Error::Source)?;

    if let Some(index) = find_eocd(&fast_buf[..fast_len]) {
        let mut eocd = [0u8; EOCD_MIN_LEN];
        eocd.copy_from_slice(&fast_buf[index..index + EOCD_MIN_LEN]);
        return Ok(eocd);
    }

    if archive_len <= FAST_EOCD_PROBE as u64 {
        return Err(Error::Archive(ArchiveError::EndOfCentralDirectoryMissing));
    }

    let tail_len = archive_len.min(MAX_EOCD_SEARCH as u64);
    let tail_len =
        usize::try_from(tail_len).map_err(|_| Error::Archive(ArchiveError::SizeOverflow))?;

    let tail_offset = archive_len
        .checked_sub(tail_len as u64)
        .ok_or(Error::Archive(ArchiveError::SizeOverflow))?;

    let mut tail = vec![0u8; tail_len];
    source
        .read_exact_at(tail_offset, &mut tail)
        .await
        .map_err(Error::Source)?;

    let index =
        find_eocd(&tail).ok_or(Error::Archive(ArchiveError::EndOfCentralDirectoryMissing))?;

    let mut eocd = [0u8; EOCD_MIN_LEN];
    eocd.copy_from_slice(&tail[index..index + EOCD_MIN_LEN]);
    Ok(eocd)
}

fn parse_central_directory<E>(
    central_directory: &[u8],
    entry_count: u16,
) -> Result<Vec<(String, IndexedEntry)>, Error<E>> {
    let mut entries = Vec::with_capacity(usize::from(entry_count));
    let mut offset = 0usize;

    for _ in 0..entry_count {
        let header_end = offset
            .checked_add(CENTRAL_HEADER_LEN)
            .ok_or(Error::Archive(ArchiveError::SizeOverflow))?;

        let header = central_directory
            .get(offset..header_end)
            .ok_or(Error::Archive(ArchiveError::InvalidCentralDirectory))?;

        if le_u32(&header[0..4]) != CENTRAL_HEADER_SIGNATURE {
            return Err(Error::Archive(ArchiveError::InvalidCentralDirectory));
        }

        let flags = le_u16(&header[8..10]);
        let compression = le_u16(&header[10..12]);
        let compressed_size = le_u32(&header[20..24]);
        let uncompressed_size = le_u32(&header[24..28]);
        let name_len = usize::from(le_u16(&header[28..30]));
        let extra_len = usize::from(le_u16(&header[30..32]));
        let comment_len = usize::from(le_u16(&header[32..34]));
        let disk_start = le_u16(&header[34..36]);
        let local_header_offset = le_u32(&header[42..46]);

        let name_end = header_end
            .checked_add(name_len)
            .ok_or(Error::Archive(ArchiveError::SizeOverflow))?;
        let next_offset = name_end
            .checked_add(extra_len)
            .and_then(|value| value.checked_add(comment_len))
            .ok_or(Error::Archive(ArchiveError::SizeOverflow))?;

        if next_offset > central_directory.len() {
            return Err(Error::Archive(ArchiveError::InvalidCentralDirectory));
        }

        let name = core::str::from_utf8(&central_directory[header_end..name_end])
            .map_err(Error::Utf8)?;

        entries.push((
            String::from(name),
            IndexedEntry {
                entry: Entry {
                    flags,
                    compression,
                    compressed_size,
                    uncompressed_size,
                    local_header_offset,
                },
                disk_start,
            },
        ));

        offset = next_offset;
    }

    // Rust's slice sort is stable, so deduplicating keeps the first central entry per name.
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    entries.dedup_by(|later, earlier| later.0 == earlier.0);

    Ok(entries)
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

#[cfg(test)]
mod tests {
    use alloc::{vec, vec::Vec};
    use core::cell::RefCell;

    use futures_lite::future;
    use inkpaper_io::{ByteSource, SliceSource, SliceSourceError};

    use super::*;

    struct RecordingSource<'a> {
        inner: SliceSource<'a>,
        reads: &'a RefCell<Vec<(u64, usize)>>,
    }

    impl ByteSource for RecordingSource<'_> {
        type Error = SliceSourceError;

        async fn len(&mut self) -> Result<u64, Self::Error> {
            self.inner.len().await
        }

        async fn read_exact_at(
            &mut self,
            offset: u64,
            buffer: &mut [u8],
        ) -> Result<(), Self::Error> {
            self.reads.borrow_mut().push((offset, buffer.len()));
            self.inner.read_exact_at(offset, buffer).await
        }
    }

    fn build_zip(entries: &[(&str, &[u8], u16)], comment: &[u8]) -> Vec<u8> {
        let mut archive = Vec::new();
        let mut central = Vec::new();

        for &(name, data, compression) in entries {
            let local_offset = u32::try_from(archive.len()).unwrap();
            let name_bytes = name.as_bytes();
            let payload: Vec<u8> = match compression {
                COMPRESSION_STORED => data.to_vec(),
                COMPRESSION_DEFLATE => miniz_oxide::deflate::compress_to_vec(data, 6),
                other => panic!("unsupported test compression {other}"),
            };
            let compressed_size = u32::try_from(payload.len()).unwrap();
            let uncompressed_size = u32::try_from(data.len()).unwrap();

            archive.extend_from_slice(&LOCAL_HEADER_SIGNATURE.to_le_bytes());
            archive.extend_from_slice(&20u16.to_le_bytes());
            archive.extend_from_slice(&0u16.to_le_bytes());
            archive.extend_from_slice(&compression.to_le_bytes());
            archive.extend_from_slice(&0u16.to_le_bytes());
            archive.extend_from_slice(&0u16.to_le_bytes());
            archive.extend_from_slice(&0u32.to_le_bytes());
            archive.extend_from_slice(&compressed_size.to_le_bytes());
            archive.extend_from_slice(&uncompressed_size.to_le_bytes());
            archive.extend_from_slice(&u16::try_from(name_bytes.len()).unwrap().to_le_bytes());
            archive.extend_from_slice(&0u16.to_le_bytes());
            archive.extend_from_slice(name_bytes);
            archive.extend_from_slice(&payload);

            central.extend_from_slice(&CENTRAL_HEADER_SIGNATURE.to_le_bytes());
            central.extend_from_slice(&20u16.to_le_bytes());
            central.extend_from_slice(&20u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&compression.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u32.to_le_bytes());
            central.extend_from_slice(&compressed_size.to_le_bytes());
            central.extend_from_slice(&uncompressed_size.to_le_bytes());
            central.extend_from_slice(&u16::try_from(name_bytes.len()).unwrap().to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u32.to_le_bytes());
            central.extend_from_slice(&local_offset.to_le_bytes());
            central.extend_from_slice(name_bytes);
        }

        let central_offset = u32::try_from(archive.len()).unwrap();
        let central_size = u32::try_from(central.len()).unwrap();
        let count = u16::try_from(entries.len()).unwrap();

        archive.extend_from_slice(&central);

        archive.extend_from_slice(&EOCD_SIGNATURE.to_le_bytes());
        archive.extend_from_slice(&0u16.to_le_bytes());
        archive.extend_from_slice(&0u16.to_le_bytes());
        archive.extend_from_slice(&count.to_le_bytes());
        archive.extend_from_slice(&count.to_le_bytes());
        archive.extend_from_slice(&central_size.to_le_bytes());
        archive.extend_from_slice(&central_offset.to_le_bytes());
        archive.extend_from_slice(&u16::try_from(comment.len()).unwrap().to_le_bytes());
        archive.extend_from_slice(comment);

        archive
    }

    fn build_stored_zip(entries: &[(&str, &[u8])], comment: &[u8]) -> Vec<u8> {
        let entries: Vec<_> = entries
            .iter()
            .map(|&(name, data)| (name, data, COMPRESSION_STORED))
            .collect();
        build_zip(&entries, comment)
    }

    #[test]
    fn fast_eocd_probe_and_cached_central_directory_avoid_repeated_reads() {
        let padding = vec![b'x'; 8 * 1024];
        let zip = build_stored_zip(&[("a.txt", b"hello"), ("b.bin", &padding)], &[]);
        let reads = RefCell::new(Vec::new());

        let mut archive = future::block_on(Archive::open(RecordingSource {
            inner: SliceSource::new(&zip),
            reads: &reads,
        }))
        .unwrap();

        // Open performs only the 512 B fast EOCD probe plus one bulk central directory read.
        assert_eq!(reads.borrow().len(), 2);
        assert_eq!(reads.borrow()[0].1, FAST_EOCD_PROBE);

        let a = ArchivePath::new("a.txt").unwrap();
        let b = ArchivePath::new("b.bin").unwrap();

        // Size queries use the in-memory index with zero I/O.
        reads.borrow_mut().clear();
        assert_eq!(
            archive.uncompressed_sizes(&[a.clone(), b.clone()]).unwrap(),
            vec![Some(5), Some(padding.len() as u64)],
        );
        assert!(reads.borrow().is_empty());

        // Reading an entry only reads its local header and payload (no central directory scan).
        let data = future::block_on(archive.read_entry(&a)).unwrap();
        assert_eq!(data, b"hello");
        assert_eq!(reads.borrow().len(), 2);
        assert_eq!(reads.borrow()[0].1, LOCAL_HEADER_LEN);
        assert_eq!(reads.borrow()[1].1, 5);
    }

    #[test]
    fn eocd_probe_falls_back_when_zip_comment_exceeds_fast_probe() {
        let comment = vec![b'c'; 1024];
        let zip = build_stored_zip(&[("note.txt", b"with long comment")], &comment);
        let reads = RefCell::new(Vec::new());

        let mut archive = future::block_on(Archive::open(RecordingSource {
            inner: SliceSource::new(&zip),
            reads: &reads,
        }))
        .unwrap();

        // Fast 512 B probe misses, fallback full-tail read finds EOCD, then 1 CD read.
        assert_eq!(reads.borrow().len(), 3);
        assert_eq!(reads.borrow()[0].1, FAST_EOCD_PROBE);

        let path = ArchivePath::new("note.txt").unwrap();
        assert_eq!(
            future::block_on(archive.read_entry(&path)).unwrap(),
            b"with long comment",
        );
    }

    #[test]
    fn duplicate_entry_names_resolve_to_first_central_entry() {
        let zip = build_stored_zip(&[("dup.txt", b"first"), ("dup.txt", b"second")], &[]);
        let mut archive = future::block_on(Archive::open(SliceSource::new(&zip))).unwrap();

        let path = ArchivePath::new("dup.txt").unwrap();
        assert_eq!(future::block_on(archive.read_entry(&path)).unwrap(), b"first");
    }

    fn make_large_png(width: u32, height: u32, trailing_bytes: usize) -> Vec<u8> {
        let mut bytes = vec![0u8; 24 + trailing_bytes];
        bytes[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
        bytes[12..16].copy_from_slice(b"IHDR");
        bytes[16..20].copy_from_slice(&width.to_be_bytes());
        bytes[20..24].copy_from_slice(&height.to_be_bytes());
        // pseudo-random trailing data so deflate stays large too
        let mut x = 0x1234_5678u32;
        for byte in &mut bytes[24..] {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            *byte = x as u8;
        }
        bytes
    }

    fn make_jpeg_with_app1(width: u16, height: u16, app1_payload_len: usize, tail: usize) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(2 + 4 + app1_payload_len + 19 + tail);
        bytes.extend_from_slice(&[0xff, 0xd8]);
        // APP1 segment (e.g. EXIF) before SOF0
        bytes.extend_from_slice(&[0xff, 0xe1]);
        let seg_len = u16::try_from(app1_payload_len + 2).unwrap();
        bytes.extend_from_slice(&seg_len.to_be_bytes());
        bytes.resize(bytes.len() + app1_payload_len, 0x42);
        // SOF0 segment
        bytes.extend_from_slice(&[0xff, 0xc0, 0x00, 0x11, 0x08]);
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&[0x03, 0x01, 0x11, 0x00, 0x02, 0x11, 0x00, 0x03, 0x11, 0x00]);
        // Scan data tail
        let mut x = 0x9e37_79b9u32;
        for _ in 0..tail {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            bytes.push(x as u8);
        }
        bytes
    }

    #[test]
    fn streaming_stored_image_headers_skips_payload_and_app_segments() {
        use crate::image::{ImageDimensions, ImageFormat, read_dimensions};

        let png = make_large_png(1200, 1600, 64 * 1024);
        let jpeg = make_jpeg_with_app1(800, 600, 40 * 1024, 64 * 1024);
        let zip = build_stored_zip(&[("pic.png", &png), ("photo.jpg", &jpeg)], &[]);

        let reads = RefCell::new(Vec::new());
        let mut archive = future::block_on(Archive::open(RecordingSource {
            inner: SliceSource::new(&zip),
            reads: &reads,
        }))
        .unwrap();

        reads.borrow_mut().clear();
        let mut png_stream = future::block_on(
            archive.open_entry_stream(&ArchivePath::new("pic.png").unwrap()),
        )
        .unwrap();
        let png_dims = future::block_on(read_dimensions(ImageFormat::Png, &mut png_stream)).unwrap();
        assert_eq!(png_dims, Some(ImageDimensions::new(1200, 1600)));
        let png_bytes_read: usize = reads.borrow().iter().map(|&(_, len)| len).sum();
        assert!(
            png_bytes_read <= LOCAL_HEADER_LEN + STREAM_STORED_BUF_LEN,
            "expected small header read, got {png_bytes_read}"
        );

        reads.borrow_mut().clear();
        let mut jpg_stream = future::block_on(
            archive.open_entry_stream(&ArchivePath::new("photo.jpg").unwrap()),
        )
        .unwrap();
        let jpg_dims = future::block_on(read_dimensions(ImageFormat::Jpeg, &mut jpg_stream)).unwrap();
        assert_eq!(jpg_dims, Some(ImageDimensions::new(800, 600)));
        let jpg_bytes_read: usize = reads.borrow().iter().map(|&(_, len)| len).sum();
        // Skipped the 40 KiB APP1 segment without reading it!
        assert!(
            jpg_bytes_read <= LOCAL_HEADER_LEN + 2 * STREAM_STORED_BUF_LEN,
            "expected APP1 skip without reading payload, got {jpg_bytes_read}"
        );
    }

    #[test]
    fn streaming_deflated_image_headers_stops_early_and_handles_ring_wrap() {
        use crate::image::{ImageDimensions, ImageFormat, read_dimensions};

        let png = make_large_png(640, 480, 64 * 1024);
        // 40 KiB APP1 exceeds the 32 KiB inflate ring buffer, exercising wraparound.
        let jpeg = make_jpeg_with_app1(1024, 768, 40 * 1024, 64 * 1024);
        let zip = build_zip(
            &[
                ("pic.png", &png, COMPRESSION_DEFLATE),
                ("photo.jpg", &jpeg, COMPRESSION_DEFLATE),
            ],
            &[],
        );

        let reads = RefCell::new(Vec::new());
        let mut archive = future::block_on(Archive::open(RecordingSource {
            inner: SliceSource::new(&zip),
            reads: &reads,
        }))
        .unwrap();

        reads.borrow_mut().clear();
        let mut png_stream = future::block_on(
            archive.open_entry_stream(&ArchivePath::new("pic.png").unwrap()),
        )
        .unwrap();
        let png_dims = future::block_on(read_dimensions(ImageFormat::Png, &mut png_stream)).unwrap();
        assert_eq!(png_dims, Some(ImageDimensions::new(640, 480)));
        let png_bytes_read: usize = reads.borrow().iter().map(|&(_, len)| len).sum();
        assert!(
            png_bytes_read < 4 * 1024,
            "expected early deflate stop on PNG header, got {png_bytes_read}"
        );

        reads.borrow_mut().clear();
        let mut jpg_stream = future::block_on(
            archive.open_entry_stream(&ArchivePath::new("photo.jpg").unwrap()),
        )
        .unwrap();
        let jpg_dims = future::block_on(read_dimensions(ImageFormat::Jpeg, &mut jpg_stream)).unwrap();
        assert_eq!(jpg_dims, Some(ImageDimensions::new(1024, 768)));
        let jpg_bytes_read: usize = reads.borrow().iter().map(|&(_, len)| len).sum();
        // Stopped before reading the 64 KiB random scan-data tail
        assert!(
            jpg_bytes_read < 16 * 1024,
            "expected early deflate stop after SOF0, got {jpg_bytes_read}"
        );
    }
}
