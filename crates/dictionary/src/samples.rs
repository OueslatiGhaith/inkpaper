use alloc::vec::Vec;
use core::cmp::Ordering;

use crate::index::compare_folded;

/// Entries between samples, crosspoint's interval. A lookup reads at most
/// this many entries after bisecting the samples.
pub(crate) const SAMPLE_INTERVAL: usize = 256;

const MAGIC: [u8; 4] = *b"IXS2";

/// Where every 256th entry of a dictionary's `.idx`, and of its `.syn` when
/// it has one, starts, along with its headword bytes, so a lookup can bisect
/// them in memory with zero I/O and read only the entries after the closest
/// one. They're taken in one pass over each file, and saved to skip those
/// passes next time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexSamples {
    pub(crate) index: FileSamples,
    pub(crate) synonyms: Option<FileSamples>,
}

/// The samples of one file: every 256th entry's byte offset and headword.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileSamples {
    /// the size of the file they were taken from
    pub len: u64,
    offsets: Vec<u32>,
    /// concatenated sample headword bytes (without NUL)
    words: Vec<u8>,
    /// end offset into `words` for each sample
    word_ends: Vec<u32>,
}

impl IndexSamples {
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = MAGIC.to_vec();

        self.index.encode(&mut bytes);

        match &self.synonyms {
            Some(synonyms) => {
                bytes.push(1);
                synonyms.encode(&mut bytes);
            }
            None => bytes.push(0),
        }

        bytes
    }

    /// Reads samples saved by [`IndexSamples::encode`], or `None` when the
    /// bytes aren't a whole, ordered set of them.
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let (magic, rest) = bytes.split_first_chunk::<4>()?;

        if *magic != MAGIC {
            return None;
        }

        let (index, rest) = FileSamples::decode(rest)?;

        let (synonyms, rest) = match rest.split_first()? {
            (0, rest) => (None, rest),
            (1, rest) => {
                FileSamples::decode(rest).map(|(synonyms, rest)| (Some(synonyms), rest))?
            }
            _ => return None,
        };

        rest.is_empty().then_some(Self { index, synonyms })
    }
}

impl FileSamples {
    pub(crate) const fn new(len: u64) -> Self {
        Self {
            len,
            offsets: Vec::new(),
            words: Vec::new(),
            word_ends: Vec::new(),
        }
    }

    pub(crate) fn push(&mut self, offset: u32, word: &[u8]) -> Option<()> {
        if word.len() > usize::from(u8::MAX) {
            return None;
        }

        let end = u32::try_from(self.words.len().checked_add(word.len())?).ok()?;
        self.offsets.push(offset);
        self.words.extend_from_slice(word);
        self.word_ends.push(end);

        Some(())
    }

    pub(crate) fn offset(&self, index: usize) -> Option<u32> {
        self.offsets.get(index).copied()
    }

    fn word(&self, index: usize) -> &[u8] {
        let start = index
            .checked_sub(1)
            .map_or(0, |prev| self.word_ends[prev] as usize);
        let end = self.word_ends[index] as usize;

        &self.words[start..end]
    }

    /// Where to start reading entries for `target`: the last sample sorting
    /// strictly before it. Entries matching it may come right before a sample
    /// that matches too, so a sample has to sort strictly before it.
    pub(crate) fn scan_start(&self, target: &[u8]) -> u64 {
        // samples[..low] sort before target, samples[high..] don't
        let mut low = 0;
        let mut high = self.offsets.len();

        while low < high {
            let middle = low + (high - low) / 2;

            if compare_folded(self.word(middle), target) == Ordering::Less {
                low = middle + 1;
            } else {
                high = middle;
            }
        }

        low.checked_sub(1)
            .map_or(0, |sample| u64::from(self.offsets[sample]))
    }

    fn encode(&self, bytes: &mut Vec<u8>) {
        bytes.extend_from_slice(&self.len.to_le_bytes());
        bytes.extend_from_slice(&(self.offsets.len() as u32).to_le_bytes());

        for (index, &offset) in self.offsets.iter().enumerate() {
            let word = self.word(index);
            bytes.extend_from_slice(&offset.to_le_bytes());
            bytes.push(word.len() as u8);
            bytes.extend_from_slice(word);
        }
    }

    fn decode(bytes: &[u8]) -> Option<(Self, &[u8])> {
        let (len, rest) = bytes.split_first_chunk::<8>()?;
        let (count, mut rest) = rest.split_first_chunk::<4>()?;

        let len = u64::from_le_bytes(*len);
        let count = u32::from_le_bytes(*count) as usize;

        // each encoded sample is at least 4 bytes (offset) + 1 byte (word_len)
        if rest.len() < count.checked_mul(5)? {
            return None;
        }

        let mut samples = Self {
            len,
            offsets: Vec::with_capacity(count),
            words: Vec::new(),
            word_ends: Vec::with_capacity(count),
        };

        for index in 0..count {
            let (offset, after_offset) = rest.split_first_chunk::<4>()?;
            let (&word_len, after_len) = after_offset.split_first()?;
            let (word, after_word) = after_len.split_at_checked(usize::from(word_len))?;
            rest = after_word;

            let offset = u32::from_le_bytes(*offset);
            if u64::from(offset) >= len || word.contains(&0) {
                return None;
            }

            if index == 0 {
                if offset != 0 {
                    return None;
                }
            } else {
                let prev_offset = samples.offsets[index - 1];
                let prev_word = samples.word(index - 1);

                if offset <= prev_offset || compare_folded(prev_word, word) == Ordering::Greater {
                    return None;
                }
            }

            samples.push(offset, word)?;
        }

        Some((samples, rest))
    }
}
