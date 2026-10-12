use alloc::string::{String, ToString};

use crate::error::InfoError;

const MAGIC: &str = "StarDict's dict ifo file";

/// What a dictionary's `.ifo` says about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictionaryInfo {
    name: String,
    word_count: u32,
    index_file_size: u32,
    same_type_sequence: Option<String>,
}

impl DictionaryInfo {
    /// Reads an `.ifo`: its magic line, then `key=value` lines.
    pub fn parse(bytes: &[u8]) -> Result<Self, InfoError> {
        let text = core::str::from_utf8(bytes).map_err(|_| InfoError::Utf8)?;
        let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);

        let mut lines = text.lines().map(str::trim);

        if lines.next() != Some(MAGIC) {
            return Err(InfoError::NotStarDict);
        }

        let mut name = None;
        let mut word_count = None;
        let mut index_file_size = None;
        let mut same_type_sequence = None;

        for line in lines {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };

            let value = value.trim();

            match key.trim() {
                "bookname" => name = Some(value.to_string()),
                "wordcount" => word_count = Some(parse_number(value, "wordcount")?),
                "idxfilesize" => index_file_size = Some(parse_number(value, "idxfilesize")?),
                "sametypesequence" if !value.is_empty() => {
                    same_type_sequence = Some(value.to_string());
                }
                "idxoffsetbits" => match value {
                    "32" => {}
                    "64" => return Err(InfoError::UnsupportedOffsetBits),
                    _ => return Err(InfoError::InvalidField("idxoffsetbits")),
                },
                _ => {}
            }
        }

        Ok(Self {
            name: name.unwrap_or_default(),
            word_count: word_count.ok_or(InfoError::MissingField("wordcount"))?,
            index_file_size: index_file_size.ok_or(InfoError::MissingField("idxfilesize"))?,
            same_type_sequence,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub const fn word_count(&self) -> u32 {
        self.word_count
    }

    pub const fn index_file_size(&self) -> u32 {
        self.index_file_size
    }

    /// The field types every definition shares, like `m` or `tm`, or `None`
    /// when each definition names its own.
    pub fn same_type_sequence(&self) -> Option<&str> {
        self.same_type_sequence.as_deref()
    }
}

fn parse_number(value: &str, field: &'static str) -> Result<u32, InfoError> {
    value.parse().map_err(|_| InfoError::InvalidField(field))
}
