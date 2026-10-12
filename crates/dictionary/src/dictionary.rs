use alloc::{string::String, vec};
use core::cmp::Ordering;

use inkpaper_epub::EpubSource;

use crate::{
    definition::{Definition, parse_fields},
    error::Error,
    index::{IndexReader, compare_folded},
    info::DictionaryInfo,
};

/// The most of one definition read, like crosspoint's cap. A longer one is
/// cut short.
const MAX_DEFINITION_BYTES: u32 = 64 * 1024;

/// An open StarDict dictionary: its `.idx` and `.dict`, read in place.
pub struct Dictionary<S> {
    info: DictionaryInfo,
    index: S,
    index_len: u64,
    data: S,
    data_len: u64,
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
        let data_len = data.len().await.map_err(Error::Source)?;

        Ok(Self {
            info,
            index,
            index_len,
            data,
            data_len,
        })
    }

    pub const fn info(&self) -> &DictionaryInfo {
        &self.info
    }

    /// The definition of `word`, matched the way StarDict sorts: ASCII
    /// letters in any case. An entry spelled exactly like `word` wins over
    /// one in another case.
    pub async fn lookup(&mut self, word: &str) -> Result<Option<Definition>, Error<S::Error>> {
        let target = word.as_bytes();

        let mut entries = IndexReader::new(&mut self.index, self.index_len, 0);
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

        self.data
            .read_exact_at(u64::from(offset), &mut data)
            .await
            .map_err(Error::Source)?;

        let fields = parse_fields(&data, self.info.same_type_sequence());

        Ok(Definition::new(headword, fields))
    }
}
