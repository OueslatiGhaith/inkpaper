use alloc::{boxed::Box, vec, vec::Vec};

use inkpaper_epub::EpubSource;
use miniz_oxide::inflate::core::{
    DecompressorOxide, decompress,
    inflate_flags::{TINFL_FLAG_HAS_MORE_INPUT, TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF},
};

use crate::error::Error;

const GZIP_MAGIC: [u8; 2] = [0x1f, 0x8b];
const DEFLATE: u8 = 8;

const FLAG_HEADER_CRC: u8 = 0x02;
const FLAG_EXTRA: u8 = 0x04;
const FLAG_NAME: u8 = 0x08;
const FLAG_COMMENT: u8 = 0x10;

/// the magic, method, flags, time, extra flags and system
const FIXED_HEADER_BYTES: u64 = 10;

/// What a gzip name or comment is read for its NUL with, at a time
const STRING_WINDOW_BYTES: usize = 256;

/// A `.dict.dz`: gzip whose header lists chunks deflated on their own, so a
/// definition is inflated from the chunks around it rather than the whole
/// file. See dictzip(1).
pub(crate) struct DictZip {
    /// the file offset of the first chunk
    data_start: u64,
    /// the bytes each chunk inflates to, the last one fewer
    chunk_len: u32,
    /// each chunk's start after `data_start`, then the end of the last
    chunk_offsets: Vec<u64>,
    /// gzip's size of the inflated data
    len: u64,
}

impl DictZip {
    /// Reads the chunk table of a gzip `source`, or `None` when it isn't gzip.
    pub async fn parse<S>(source: &mut S, file_len: u64) -> Result<Option<Self>, Error<S::Error>>
    where
        S: EpubSource,
    {
        // the fixed header, the extra field's length, and the inflated size at the end
        if file_len < FIXED_HEADER_BYTES + 2 + 4 {
            return Ok(None);
        }

        let mut header = [0; FIXED_HEADER_BYTES as usize + 2];
        read(source, 0, &mut header).await?;

        if header[..2] != GZIP_MAGIC {
            return Ok(None);
        }

        let flags = header[3];

        if header[2] != DEFLATE || flags & FLAG_EXTRA == 0 {
            return Err(Error::MalformedDictZip);
        }

        let extra_len = u16::from_le_bytes([header[10], header[11]]);
        let mut extra = vec![0; usize::from(extra_len)];
        read(source, FIXED_HEADER_BYTES + 2, &mut extra).await?;

        let (chunk_len, chunk_sizes) =
            random_access_field(&extra).ok_or(Error::MalformedDictZip)?;

        let mut data_start = FIXED_HEADER_BYTES + 2 + u64::from(extra_len);

        for flag in [FLAG_NAME, FLAG_COMMENT] {
            if flags & flag != 0 {
                data_start = skip_string(source, file_len, data_start).await?;
            }
        }

        if flags & FLAG_HEADER_CRC != 0 {
            data_start += 2;
        }

        let mut chunk_offsets = Vec::with_capacity(chunk_sizes.len() / 2 + 1);
        let mut offset = 0u64;

        chunk_offsets.push(offset);

        for size in chunk_sizes.as_chunks::<2>().0 {
            offset += u64::from(u16::from_le_bytes(*size));
            chunk_offsets.push(offset);
        }

        if data_start + offset > file_len {
            return Err(Error::MalformedDictZip);
        }

        let mut size = [0; 4];
        read(source, file_len - 4, &mut size).await?;

        Ok(Some(Self {
            data_start,
            chunk_len: u32::from(chunk_len),
            chunk_offsets,
            len: u64::from(u32::from_le_bytes(size)),
        }))
    }

    pub const fn len(&self) -> u64 {
        self.len
    }

    /// Inflates the chunks holding `out.len()` bytes from `offset` into `out`.
    pub async fn read<S>(
        &self,
        source: &mut S,
        offset: u64,
        out: &mut [u8],
    ) -> Result<(), Error<S::Error>>
    where
        S: EpubSource,
    {
        if out.is_empty() {
            return Ok(());
        }

        let chunk_len = u64::from(self.chunk_len);
        let end = offset + out.len() as u64;

        let mut decompressor = Box::<DecompressorOxide>::default();
        let mut compressed = Vec::new();
        let mut chunk = vec![0; self.chunk_len as usize];

        for index in offset / chunk_len..=(end - 1) / chunk_len {
            let index = index as usize;

            let (Some(&start), Some(&next)) = (
                self.chunk_offsets.get(index),
                self.chunk_offsets.get(index + 1),
            ) else {
                return Err(Error::MalformedDictZip);
            };

            compressed.resize((next - start) as usize, 0);
            read(source, self.data_start + start, &mut compressed).await?;

            // a chunk ends in a full flush, not the stream's last block, so it
            // inflates to its end wanting more
            decompressor.init();

            let (status, _, inflated) = decompress(
                &mut decompressor,
                &compressed,
                &mut chunk,
                0,
                TINFL_FLAG_HAS_MORE_INPUT | TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF,
            );

            if (status as i8) < 0 {
                return Err(Error::Inflate);
            }

            let chunk_start = index as u64 * chunk_len;
            let from = offset.max(chunk_start);
            let to = end.min(chunk_start + chunk_len);

            if to > chunk_start + inflated as u64 {
                return Err(Error::Inflate);
            }

            out[(from - offset) as usize..(to - offset) as usize].copy_from_slice(
                &chunk[(from - chunk_start) as usize..(to - chunk_start) as usize],
            );
        }

        Ok(())
    }
}

/// The chunk length and the chunks' deflated sizes from the `RA` subfield of
/// a gzip extra field.
fn random_access_field(mut extra: &[u8]) -> Option<(u16, &[u8])> {
    while let Some((&[id_1, id_2, len_1, len_2], rest)) = extra.split_first_chunk::<4>() {
        let len = usize::from(u16::from_le_bytes([len_1, len_2]));
        let data = rest.get(..len)?;

        extra = &rest[len..];

        if [id_1, id_2] != *b"RA" {
            continue;
        }

        let (
            &[
                version_1,
                version_2,
                chunk_len_1,
                chunk_len_2,
                count_1,
                count_2,
            ],
            sizes,
        ) = data.split_first_chunk::<6>()?;

        let chunk_len = u16::from_le_bytes([chunk_len_1, chunk_len_2]);
        let count = usize::from(u16::from_le_bytes([count_1, count_2]));

        if u16::from_le_bytes([version_1, version_2]) != 1 || chunk_len == 0 {
            return None;
        }

        return Some((chunk_len, sizes.get(..count * 2)?));
    }

    None
}

/// The offset after the NUL ending the string at `offset`.
async fn skip_string<S>(
    source: &mut S,
    file_len: u64,
    mut offset: u64,
) -> Result<u64, Error<S::Error>>
where
    S: EpubSource,
{
    let mut window = [0; STRING_WINDOW_BYTES];

    loop {
        let len = (file_len.saturating_sub(offset)).min(STRING_WINDOW_BYTES as u64) as usize;

        if len == 0 {
            return Err(Error::MalformedDictZip);
        }

        read(source, offset, &mut window[..len]).await?;

        if let Some(end) = window[..len].iter().position(|&byte| byte == 0) {
            return Ok(offset + end as u64 + 1);
        }

        offset += len as u64;
    }
}

async fn read<S>(source: &mut S, offset: u64, buffer: &mut [u8]) -> Result<(), Error<S::Error>>
where
    S: EpubSource,
{
    source
        .read_exact_at(offset, buffer)
        .await
        .map_err(Error::Source)
}
