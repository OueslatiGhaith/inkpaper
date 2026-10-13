use alloc::vec::Vec;

/// Entries between samples, crosspoint's interval. A lookup reads at most
/// this many entries after bisecting the samples.
pub(crate) const SAMPLE_INTERVAL: usize = 256;

const MAGIC: [u8; 4] = *b"IXS1";

/// Where every 256th entry of a dictionary's `.idx`, and of its `.syn` when
/// it has one, starts, so a lookup can bisect them and read only the entries
/// after the closest one. They're taken in one pass over each file, and saved
/// to skip those passes next time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexSamples {
    pub(crate) index: FileSamples,
    pub(crate) synonyms: Option<FileSamples>,
}

/// The samples of one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileSamples {
    /// the size of the file they were taken from
    pub len: u64,
    pub offsets: Vec<u32>,
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
    fn encode(&self, bytes: &mut Vec<u8>) {
        bytes.extend_from_slice(&self.len.to_le_bytes());
        bytes.extend_from_slice(&(self.offsets.len() as u32).to_le_bytes());

        for offset in &self.offsets {
            bytes.extend_from_slice(&offset.to_le_bytes());
        }
    }

    fn decode(bytes: &[u8]) -> Option<(Self, &[u8])> {
        let (len, rest) = bytes.split_first_chunk::<8>()?;
        let (count, rest) = rest.split_first_chunk::<4>()?;

        let len = u64::from_le_bytes(*len);
        let count = u32::from_le_bytes(*count) as usize;

        let (offsets, rest) = rest.split_at_checked(count.checked_mul(4)?)?;

        let offsets: Vec<u32> = offsets
            .as_chunks::<4>()
            .0
            .iter()
            .map(|&offset| u32::from_le_bytes(offset))
            .collect();

        let ordered = offsets.windows(2).all(|pair| pair[0] < pair[1]);
        let in_bounds = offsets.last().is_none_or(|&last| u64::from(last) < len);

        if offsets.first().is_some_and(|&first| first != 0) || !ordered || !in_bounds {
            return None;
        }

        Some((Self { len, offsets }, rest))
    }
}
