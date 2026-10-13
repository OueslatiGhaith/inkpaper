use alloc::{string::String, vec};

use inkpaper_epub::EpubSource;

use crate::{
    definition::{Definition, parse_fields},
    dictzip::DictZip,
    error::Error,
    forms::word_forms,
    index::{EntryKind, WordList},
    info::DictionaryInfo,
    samples::{FileSamples, IndexSamples},
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
    S: EpubSource,
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

        self.index.set_samples(samples.index.offsets);

        if let (Some(list), Some(samples)) = (&mut self.synonyms, samples.synonyms) {
            list.set_samples(samples.offsets);
        }

        true
    }

    /// Takes samples in one pass over the index, and one over the synonyms,
    /// and looks words up through them from now on. Returns them to save.
    pub async fn sample_index(&mut self) -> Result<IndexSamples, Error<S::Error>> {
        let index = FileSamples {
            len: self.index.len(),
            offsets: self.index.sample().await?.to_vec(),
        };

        let synonyms = match &mut self.synonyms {
            Some(list) => Some(FileSamples {
                len: list.len(),
                offsets: list.sample().await?.to_vec(),
            }),
            None => None,
        };

        Ok(IndexSamples { index, synonyms })
    }

    /// The definition of `word` as a page gives it: without the punctuation
    /// around it, then as another spelling the dictionary lists, then by the
    /// English forms it could be inflected from, like `walk` for `walking`.
    pub async fn lookup(&mut self, word: &str) -> Result<Option<Definition>, Error<S::Error>> {
        for form in word_forms(word) {
            if let Some((headword, offset, size)) = self.locate(form.as_bytes()).await? {
                return self.read_definition(headword, offset, size).await.map(Some);
            }
        }

        Ok(None)
    }

    /// The headword spelled like `target`, or that `target` is a synonym of,
    /// and where its definition is.
    async fn locate(
        &mut self,
        target: &[u8],
    ) -> Result<Option<(String, u32, u32)>, Error<S::Error>> {
        if let Some((headword, tail)) = self.index.find(target).await? {
            let (offset, size) = tail.location();

            return Ok(Some((headword, offset, size)));
        }

        let Some(synonyms) = &mut self.synonyms else {
            return Ok(None);
        };

        let Some((_, synonym)) = synonyms.find(target).await? else {
            return Ok(None);
        };

        let (headword, tail) = self.index.entry(synonym.ordinal()).await?;
        let (offset, size) = tail.location();

        Ok(Some((headword, offset, size)))
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
