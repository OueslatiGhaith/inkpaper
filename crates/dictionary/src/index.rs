use alloc::vec::Vec;
use core::cmp::Ordering;

use inkpaper_epub::EpubSource;

use crate::error::Error;

/// The longest word the format allows, with its NUL
const MAX_WORD_BYTES: usize = 256;

/// the big-endian offset and size after each word
const ENTRY_TAIL_BYTES: usize = 8;

const MAX_ENTRY_BYTES: usize = MAX_WORD_BYTES + ENTRY_TAIL_BYTES;

const BUFFER_BYTES: usize = 4096;

/// One `.idx` entry: a word and where its definition sits in the `.dict`.
pub(crate) struct IndexEntry<'a> {
    pub word: &'a [u8],
    pub offset: u32,
    pub size: u32,
}

/// Reads `.idx` entries in order from `position`, a buffer at a time.
pub(crate) struct IndexReader<'s, S> {
    source: &'s mut S,
    len: u64,
    buffer: Vec<u8>,
    /// the file offset of the buffer's first byte
    buffer_start: u64,
    /// the file offset of the next entry
    position: u64,
}

impl<'s, S> IndexReader<'s, S>
where
    S: EpubSource,
{
    pub fn new(source: &'s mut S, len: u64, position: u64) -> Self {
        Self {
            source,
            len,
            buffer: Vec::new(),
            buffer_start: 0,
            position,
        }
    }

    pub async fn next(&mut self) -> Result<Option<IndexEntry<'_>>, Error<S::Error>> {
        if self.position >= self.len {
            return Ok(None);
        }

        let buffer_end = self.buffer_start + self.buffer.len() as u64;
        let entry_end = (self.position + MAX_ENTRY_BYTES as u64).min(self.len);

        // refill whenever the longest possible entry might not fit what's left
        if self.position < self.buffer_start || entry_end > buffer_end {
            let read_len = (self.len - self.position).min(BUFFER_BYTES as u64) as usize;

            self.buffer.resize(read_len, 0);
            self.source
                .read_exact_at(self.position, &mut self.buffer)
                .await
                .map_err(Error::Source)?;
            self.buffer_start = self.position;
        }

        let start = (self.position - self.buffer_start) as usize;
        let window = &self.buffer[start..];

        let word_len = window
            .iter()
            .take(MAX_WORD_BYTES)
            .position(|&byte| byte == 0)
            .ok_or(Error::MalformedIndex)?;

        let tail = window
            .get(word_len + 1..word_len + 1 + ENTRY_TAIL_BYTES)
            .ok_or(Error::MalformedIndex)?;

        let offset = u32::from_be_bytes([tail[0], tail[1], tail[2], tail[3]]);
        let size = u32::from_be_bytes([tail[4], tail[5], tail[6], tail[7]]);

        self.position += (word_len + 1 + ENTRY_TAIL_BYTES) as u64;

        Ok(Some(IndexEntry {
            word: &self.buffer[start..start + word_len],
            offset,
            size,
        }))
    }
}

/// How StarDict sorts its index first: ASCII letters compared without case,
/// every other byte as it is.
pub(crate) fn compare_folded(left: &[u8], right: &[u8]) -> Ordering {
    let left = left.iter().map(u8::to_ascii_lowercase);
    let right = right.iter().map(u8::to_ascii_lowercase);

    left.cmp(right)
}
