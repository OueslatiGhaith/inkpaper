use alloc::vec::Vec;

/// Entries between samples, crosspoint's interval. A lookup reads at most
/// this many entries after bisecting the samples.
pub(crate) const SAMPLE_INTERVAL: usize = 256;

const MAGIC: [u8; 4] = *b"IXS1";

const HEADER_BYTES: usize = MAGIC.len() + 8 + 4;

/// Where every 256th `.idx` entry starts, so a lookup can bisect them and
/// read only the entries after the closest one. It's built in one pass over
/// the index, and saved to skip that pass next time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexSamples {
    /// the size of the `.idx` the samples were taken from
    index_len: u64,
    offsets: Vec<u32>,
}

impl IndexSamples {
    pub(crate) const fn new(index_len: u64, offsets: Vec<u32>) -> Self {
        Self { index_len, offsets }
    }

    pub(crate) const fn index_len(&self) -> u64 {
        self.index_len
    }

    pub(crate) fn offsets(&self) -> &[u32] {
        &self.offsets
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(HEADER_BYTES + self.offsets.len() * 4);

        bytes.extend_from_slice(&MAGIC);
        bytes.extend_from_slice(&self.index_len.to_le_bytes());
        bytes.extend_from_slice(&(self.offsets.len() as u32).to_le_bytes());

        for offset in &self.offsets {
            bytes.extend_from_slice(&offset.to_le_bytes());
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

        let (index_len, rest) = rest.split_first_chunk::<8>()?;
        let (count, rest) = rest.split_first_chunk::<4>()?;

        let index_len = u64::from_le_bytes(*index_len);
        let count = u32::from_le_bytes(*count) as usize;

        if rest.len() != count.checked_mul(4)? {
            return None;
        }

        let offsets: Vec<u32> = rest
            .as_chunks::<4>()
            .0
            .iter()
            .map(|&offset| u32::from_le_bytes(offset))
            .collect();

        let ordered = offsets.windows(2).all(|pair| pair[0] < pair[1]);
        let in_bounds = offsets
            .last()
            .is_none_or(|&last| u64::from(last) < index_len);

        if offsets.first().is_some_and(|&first| first != 0) || !ordered || !in_bounds {
            return None;
        }

        Some(Self::new(index_len, offsets))
    }
}
