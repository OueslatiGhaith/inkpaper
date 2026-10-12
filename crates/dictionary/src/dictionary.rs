use alloc::{string::String, vec, vec::Vec};
use core::cmp::Ordering;

use inkpaper_epub::EpubSource;

use crate::{
    definition::{Definition, parse_fields},
    dictzip::DictZip,
    error::Error,
    index::{BUFFER_BYTES, IndexReader, MAX_ENTRY_BYTES, compare_folded},
    info::DictionaryInfo,
    samples::{IndexSamples, SAMPLE_INTERVAL},
};

/// The most of one definition read, like crosspoint's cap. A longer one is
/// cut short.
const MAX_DEFINITION_BYTES: u32 = 64 * 1024;

/// An open StarDict dictionary: its `.idx` and its `.dict` or `.dict.dz`,
/// read in place.
pub struct Dictionary<S> {
    info: DictionaryInfo,
    index: S,
    index_len: u64,
    data: S,
    /// the definitions' size, inflated for a `.dict.dz`
    data_len: u64,
    /// the chunk table of a `.dict.dz`
    dictzip: Option<DictZip>,
    /// without them a lookup reads the index from its start
    samples: Option<IndexSamples>,
}

impl<S> Dictionary<S>
where
    S: EpubSource,
{
    pub async fn open(
        info: DictionaryInfo,
        mut index: S,
        mut data: S,
    ) -> Result<Self, Error<S::Error>> {
        let index_len = index.len().await.map_err(Error::Source)?;
        let file_len = data.len().await.map_err(Error::Source)?;

        let dictzip = DictZip::parse(&mut data, file_len).await?;
        let data_len = dictzip.as_ref().map_or(file_len, DictZip::len);

        Ok(Self {
            info,
            index,
            index_len,
            data,
            data_len,
            dictzip,
            samples: None,
        })
    }

    pub const fn info(&self) -> &DictionaryInfo {
        &self.info
    }

    /// Looks words up through `samples`, unless they were taken from another
    /// index. Returns whether they're used.
    pub fn use_samples(&mut self, samples: IndexSamples) -> bool {
        if samples.index_len() != self.index_len {
            return false;
        }

        self.samples = Some(samples);

        true
    }

    /// Takes samples in one pass over the index and looks words up through
    /// them from now on. Returns them to save.
    pub async fn sample_index(&mut self) -> Result<&IndexSamples, Error<S::Error>> {
        let mut entries = IndexReader::new(&mut self.index, self.index_len, 0, BUFFER_BYTES);
        let mut offsets = Vec::new();
        let mut count = 0usize;

        loop {
            let position = entries.position();

            if entries.next().await?.is_none() {
                break;
            }

            if count.is_multiple_of(SAMPLE_INTERVAL) {
                offsets.push(u32::try_from(position).map_err(|_| Error::MalformedIndex)?);
            }

            count += 1;
        }

        Ok(self
            .samples
            .insert(IndexSamples::new(self.index_len, offsets)))
    }

    /// The definition of `word`, matched the way StarDict sorts: ASCII
    /// letters in any case. An entry spelled exactly like `word` wins over
    /// one in another case.
    pub async fn lookup(&mut self, word: &str) -> Result<Option<Definition>, Error<S::Error>> {
        let target = word.as_bytes();
        let start = self.scan_start(target).await?;

        let mut entries = IndexReader::new(&mut self.index, self.index_len, start, BUFFER_BYTES);
        let mut found = None;

        while let Some(entry) = entries.next().await? {
            match compare_folded(entry.word, target) {
                Ordering::Less => {}
                Ordering::Greater => break,
                Ordering::Equal => {
                    let exact = entry.word == target;

                    if found.is_none() || exact {
                        let headword = String::from_utf8_lossy(entry.word).into_owned();

                        found = Some((headword, entry.offset, entry.size));
                    }

                    if exact {
                        break;
                    }
                }
            }
        }

        let Some((headword, offset, size)) = found else {
            return Ok(None);
        };

        self.read_definition(headword, offset, size).await.map(Some)
    }

    /// Where to start reading entries for `target`: the last sample sorting
    /// before it. Entries matching it may come right before a sample that
    /// matches too, so a sample has to sort strictly before it.
    async fn scan_start(&mut self, target: &[u8]) -> Result<u64, Error<S::Error>> {
        let Some(samples) = &self.samples else {
            return Ok(0);
        };

        let offsets = samples.offsets();

        // offsets[..low] sort before target, offsets[high..] don't
        let mut low = 0;
        let mut high = offsets.len();

        while low < high {
            let middle = low + (high - low) / 2;

            let offset = u64::from(offsets[middle]);
            let mut entries =
                IndexReader::new(&mut self.index, self.index_len, offset, MAX_ENTRY_BYTES);
            let entry = entries.next().await?.ok_or(Error::MalformedIndex)?;

            if compare_folded(entry.word, target) == Ordering::Less {
                low = middle + 1;
            } else {
                high = middle;
            }
        }

        Ok(low
            .checked_sub(1)
            .map_or(0, |sample| u64::from(offsets[sample])))
    }

    async fn read_definition(
        &mut self,
        headword: String,
        offset: u32,
        size: u32,
    ) -> Result<Definition, Error<S::Error>> {
        if u64::from(offset) + u64::from(size) > self.data_len {
            return Err(Error::DefinitionOutOfBounds);
        }

        let mut data = vec![0; size.min(MAX_DEFINITION_BYTES) as usize];

        match &self.dictzip {
            Some(dictzip) => {
                dictzip
                    .read(&mut self.data, u64::from(offset), &mut data)
                    .await?
            }
            None => self
                .data
                .read_exact_at(u64::from(offset), &mut data)
                .await
                .map_err(Error::Source)?,
        }

        let fields = parse_fields(&data, self.info.same_type_sequence());

        Ok(Definition::new(headword, fields))
    }
}
