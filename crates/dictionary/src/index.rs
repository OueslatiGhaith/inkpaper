use alloc::{string::String, vec::Vec};
use core::cmp::Ordering;

use inkpaper_io::ByteSource;

use crate::{
    error::Error,
    samples::{FileSamples, SAMPLE_INTERVAL},
};

/// The longest word the format allows, with its NUL
const MAX_WORD_BYTES: usize = 256;

/// The longest entry, all a reader of one entry needs to read
const MAX_ENTRY_BYTES: usize = MAX_WORD_BYTES + 8;

/// What a reader of many entries reads at a time
const BUFFER_BYTES: usize = 4096;

/// The files listing words in StarDict's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EntryKind {
    /// `.idx`: each word, then the big-endian offset and size of its
    /// definition
    Index,
    /// `.syn`: each word, then the big-endian number of the `.idx` entry it's
    /// another spelling of
    Synonym,
}

impl EntryKind {
    const fn tail_bytes(self) -> usize {
        match self {
            Self::Index => 8,
            Self::Synonym => 4,
        }
    }
}

/// The bytes after an entry's word.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Tail([u8; 8]);

impl Tail {
    /// An `.idx` entry's definition offset and size.
    pub const fn location(self) -> (u32, u32) {
        let [a, b, c, d, e, f, g, h] = self.0;

        (
            u32::from_be_bytes([a, b, c, d]),
            u32::from_be_bytes([e, f, g, h]),
        )
    }

    /// The number of the `.idx` entry a `.syn` entry spells another way.
    pub const fn ordinal(self) -> u32 {
        let [a, b, c, d, ..] = self.0;

        u32::from_be_bytes([a, b, c, d])
    }
}

struct Entry<'a> {
    word: &'a [u8],
    tail: Tail,
}

/// An `.idx` or `.syn`, searched in place.
pub(crate) struct WordList<S> {
    source: S,
    len: u64,
    kind: EntryKind,
    /// where every 256th entry starts and its headword; without them a search
    /// reads the file from its start
    samples: Option<FileSamples>,
}

impl<S> WordList<S>
where
    S: ByteSource,
{
    pub async fn open(mut source: S, kind: EntryKind) -> Result<Self, Error<S::Error>> {
        let len = source.len().await.map_err(Error::Source)?;

        Ok(Self {
            source,
            len,
            kind,
            samples: None,
        })
    }

    pub const fn len(&self) -> u64 {
        self.len
    }

    pub fn set_samples(&mut self, samples: FileSamples) {
        self.samples = Some(samples);
    }

    /// Takes samples in one pass over the file and searches through them from
    /// now on.
    pub async fn sample(&mut self) -> Result<&FileSamples, Error<S::Error>> {
        let mut entries = EntryReader::new(&mut self.source, self.len, self.kind, 0);
        let mut samples = FileSamples::new(self.len);
        let mut count = 0usize;

        loop {
            let position = entries.position();

            let Some(entry) = entries.next().await? else {
                break;
            };

            if count.is_multiple_of(SAMPLE_INTERVAL) {
                let offset = u32::try_from(position).map_err(|_| Error::MalformedIndex)?;
                samples
                    .push(offset, entry.word)
                    .ok_or(Error::MalformedIndex)?;
            }

            count += 1;
        }

        Ok(self.samples.insert(samples))
    }

    /// All entries matching `target` the way StarDict sorts, ASCII letters in
    /// any case. When one or more entries match `target`'s exact case, only
    /// those exact-case entries are returned; otherwise all case-folded
    /// matches are returned in index order.
    pub async fn find_all(
        &mut self,
        target: &[u8],
    ) -> Result<Vec<(String, Tail)>, Error<S::Error>> {
        let start = self.scan_start(target);

        let mut entries = EntryReader::new(&mut self.source, self.len, self.kind, start);
        let mut exact = Vec::new();
        let mut folded = Vec::new();

        while let Some(entry) = entries.next().await? {
            match compare_folded(entry.word, target) {
                Ordering::Less => {}
                Ordering::Greater => break,
                Ordering::Equal => {
                    if entry.word == target {
                        exact.push((
                            String::from_utf8_lossy(entry.word).into_owned(),
                            entry.tail,
                        ));
                    } else if exact.is_empty() {
                        folded.push((
                            String::from_utf8_lossy(entry.word).into_owned(),
                            entry.tail,
                        ));
                    }
                }
            }
        }

        if exact.is_empty() {
            Ok(folded)
        } else {
            Ok(exact)
        }
    }

    /// The entry numbered `ordinal`, counting from 0.
    pub async fn entry(&mut self, ordinal: u32) -> Result<(String, Tail), Error<S::Error>> {
        let ordinal = ordinal as usize;

        let (start, skip) = match &self.samples {
            Some(samples) => {
                let sample = samples
                    .offset(ordinal / SAMPLE_INTERVAL)
                    .ok_or(Error::MalformedIndex)?;

                (u64::from(sample), ordinal % SAMPLE_INTERVAL)
            }
            None => (0, ordinal),
        };

        let mut entries = EntryReader::new(&mut self.source, self.len, self.kind, start);

        for _ in 0..skip {
            entries.next().await?.ok_or(Error::MalformedIndex)?;
        }

        let entry = entries.next().await?.ok_or(Error::MalformedIndex)?;

        Ok((String::from_utf8_lossy(entry.word).into_owned(), entry.tail))
    }

    /// Where to start reading entries for `target`, computed purely in memory
    /// from the sampled headwords.
    fn scan_start(&self, target: &[u8]) -> u64 {
        self.samples
            .as_ref()
            .map_or(0, |samples| samples.scan_start(target))
    }
}

/// Reads entries in order from `position`, a buffer at a time.
struct EntryReader<'s, S> {
    source: &'s mut S,
    len: u64,
    kind: EntryKind,
    buffer: Vec<u8>,
    /// the file offset of the buffer's first byte
    buffer_start: u64,
    /// the file offset of the next entry
    position: u64,
}

impl<'s, S> EntryReader<'s, S>
where
    S: ByteSource,
{
    fn new(source: &'s mut S, len: u64, kind: EntryKind, position: u64) -> Self {
        Self {
            source,
            len,
            kind,
            buffer: Vec::new(),
            buffer_start: 0,
            position,
        }
    }

    const fn position(&self) -> u64 {
        self.position
    }

    async fn next(&mut self) -> Result<Option<Entry<'_>>, Error<S::Error>> {
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

        let tail_bytes = self.kind.tail_bytes();
        let tail = window
            .get(word_len + 1..word_len + 1 + tail_bytes)
            .ok_or(Error::MalformedIndex)?;

        let mut padded = [0; 8];
        padded[..tail_bytes].copy_from_slice(tail);

        self.position += (word_len + 1 + tail_bytes) as u64;

        Ok(Some(Entry {
            word: &self.buffer[start..start + word_len],
            tail: Tail(padded),
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
