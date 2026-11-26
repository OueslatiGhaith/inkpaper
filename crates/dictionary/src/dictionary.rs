use alloc::{string::String, vec::Vec};

use inkpaper_io::ByteSource;

use crate::{
    definition::{Definition, parse_fields},
    dictzip::DictZip,
    error::Error,
    forms::word_forms,
    index::{EntryKind, WordList},
    info::DictionaryInfo,
    samples::IndexSamples,
};

/// The most of one definition read, like crosspoint's cap. A longer one is
/// cut short.
const MAX_DEFINITION_BYTES: u32 = 64 * 1024;

/// An open StarDict dictionary: its `.idx`, its `.dict` or `.dict.dz`, and
/// its `.syn` when it has one, read in place.
pub struct Dictionary<S> {
    info: DictionaryInfo,
    index: WordList<S>,
    /// other spellings of the index's words, like `oxen` for `ox`
    synonyms: Option<WordList<S>>,
    data: S,
    /// the definitions' size, inflated for a `.dict.dz`
    data_len: u64,
    /// the chunk table of a `.dict.dz`
    dictzip: Option<DictZip>,
}

impl<S> Dictionary<S>
where
    S: ByteSource,
{
    pub async fn open(
        info: DictionaryInfo,
        index: S,
        mut data: S,
    ) -> Result<Self, Error<S::Error>> {
        let index = WordList::open(index, EntryKind::Index).await?;
        let file_len = data.len().await.map_err(Error::Source)?;

        let dictzip = DictZip::parse(&mut data, file_len).await?;
        let data_len = dictzip.as_ref().map_or(file_len, DictZip::len);

        Ok(Self {
            info,
            index,
            synonyms: None,
            data,
            data_len,
            dictzip,
        })
    }

    pub const fn info(&self) -> &DictionaryInfo {
        &self.info
    }

    /// Looks words up in the dictionary's `.syn` too, before trying the
    /// English forms they could come from. Samples cover it when they're
    /// taken or used after this.
    pub async fn use_synonyms(&mut self, synonyms: S) -> Result<(), Error<S::Error>> {
        self.synonyms = Some(WordList::open(synonyms, EntryKind::Synonym).await?);

        Ok(())
    }

    /// Looks words up through `samples`, unless they were taken from other
    /// files. Returns whether they're used.
    pub fn use_samples(&mut self, samples: IndexSamples) -> bool {
        let synonyms_match = match (&self.synonyms, &samples.synonyms) {
            (Some(list), Some(samples)) => list.len() == samples.len,
            (None, None) => true,
            _ => false,
        };

        if samples.index.len != self.index.len() || !synonyms_match {
            return false;
        }

        self.index.set_samples(samples.index);

        if let (Some(list), Some(samples)) = (&mut self.synonyms, samples.synonyms) {
            list.set_samples(samples);
        }

        true
    }

    /// Takes samples in one pass over the index, and one over the synonyms,
    /// and looks words up through them from now on. Returns them to save.
    pub async fn sample_index(&mut self) -> Result<IndexSamples, Error<S::Error>> {
        let index = self.index.sample().await?.clone();

        let synonyms = match &mut self.synonyms {
            Some(list) => Some(list.sample().await?.clone()),
            None => None,
        };

        Ok(IndexSamples { index, synonyms })
    }

    /// The definition of `word` as a page gives it: without the punctuation
    /// around it, then as another spelling the dictionary lists, then by the
    /// English forms it could be inflected from, like `walk` for `walking`.
    /// Homographs matching the same headword are combined in index order.
    pub async fn lookup(&mut self, word: &str) -> Result<Option<Definition>, Error<S::Error>> {
        for form in word_forms(word) {
            let matches = self.locate(form.as_bytes()).await?;
            if !matches.is_empty() {
                return self.read_definitions(matches).await;
            }
        }

        Ok(None)
    }

    /// All headwords spelled like `target`, or that `target` is a synonym of,
    /// and where their definitions are.
    async fn locate(&mut self, target: &[u8]) -> Result<Vec<(String, u32, u32)>, Error<S::Error>> {
        let index_matches = self.index.find_all(target).await?;
        if !index_matches.is_empty() {
            let mut results = Vec::with_capacity(index_matches.len());

            for (headword, tail) in index_matches {
                let (offset, size) = tail.location();
                push_unique_location(&mut results, headword, offset, size);
            }

            return Ok(results);
        }

        let Some(synonyms) = &mut self.synonyms else {
            return Ok(Vec::new());
        };

        let syn_matches = synonyms.find_all(target).await?;
        let mut results = Vec::with_capacity(syn_matches.len());

        for (_, synonym) in syn_matches {
            let (headword, tail) = self.index.entry(synonym.ordinal()).await?;
            let (offset, size) = tail.location();
            push_unique_location(&mut results, headword, offset, size);
        }

        Ok(results)
    }

    async fn read_definitions(
        &mut self,
        matches: Vec<(String, u32, u32)>,
    ) -> Result<Option<Definition>, Error<S::Error>> {
        let mut matches = matches.into_iter();
        let Some((headword, first_offset, first_size)) = matches.next() else {
            return Ok(None);
        };

        let mut fields = Vec::new();
        let mut remaining = MAX_DEFINITION_BYTES;
        let mut data = Vec::new();

        for (offset, size) in core::iter::once((first_offset, first_size))
            .chain(matches.map(|(_, offset, size)| (offset, size)))
        {
            if u64::from(offset) + u64::from(size) > self.data_len {
                return Err(Error::DefinitionOutOfBounds);
            }

            if remaining == 0 {
                break;
            }

            let read_len = size.min(remaining);
            remaining -= read_len;

            data.resize(read_len as usize, 0);

            match &mut self.dictzip {
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

            fields.extend(parse_fields(&data, self.info.same_type_sequence()));
        }

        Ok(Some(Definition::new(headword, fields)))
    }
}

fn push_unique_location(
    results: &mut Vec<(String, u32, u32)>,
    headword: String,
    offset: u32,
    size: u32,
) {
    if !results
        .iter()
        .any(|&(_, existing_offset, existing_size)| {
            existing_offset == offset && existing_size == size
        })
    {
        results.push((headword, offset, size));
    }
}
