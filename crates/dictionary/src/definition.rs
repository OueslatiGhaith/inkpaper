use alloc::{
    string::{String, ToString},
    vec::Vec,
};

/// A word's entry: the headword as the dictionary spells it, and the text
/// fields of its definition in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Definition {
    headword: String,
    fields: Vec<DefinitionField>,
}

impl Definition {
    pub(crate) const fn new(headword: String, fields: Vec<DefinitionField>) -> Self {
        Self { headword, fields }
    }

    pub fn headword(&self) -> &str {
        &self.headword
    }

    pub fn fields(&self) -> &[DefinitionField] {
        &self.fields
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionField {
    kind: FieldKind,
    text: String,
}

impl DefinitionField {
    pub const fn kind(&self) -> FieldKind {
        self.kind
    }

    pub fn text(&self) -> &str {
        &self.text
    }
}

/// The text field types StarDict defines. Its binary ones, sounds and
/// pictures, are skipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    /// `m`, and `l` in the locale's encoding, read as UTF-8 here
    Plain,
    /// `t` and `y`, a pronunciation
    Phonetic,
    /// `g`
    Pango,
    /// `x`
    Xdxf,
    /// `h`
    Html,
    /// `k`
    KingSoft,
    /// `w`
    MediaWiki,
}

impl FieldKind {
    fn from_type(kind: u8) -> Option<Self> {
        match kind {
            b'm' | b'l' => Some(Self::Plain),
            b't' | b'y' => Some(Self::Phonetic),
            b'g' => Some(Self::Pango),
            b'x' => Some(Self::Xdxf),
            b'h' => Some(Self::Html),
            b'k' => Some(Self::KingSoft),
            b'w' => Some(Self::MediaWiki),
            _ => None,
        }
    }
}

/// Splits a definition's data into its fields. With `same_type_sequence`
/// the types aren't stored and the last field runs to the end; without it
/// each field starts with its type. Data cut short keeps the fields read.
pub(crate) fn parse_fields(data: &[u8], same_type_sequence: Option<&str>) -> Vec<DefinitionField> {
    let mut fields = Vec::new();
    let mut rest = data;

    match same_type_sequence {
        Some(types) => {
            let types = types.as_bytes();

            for (index, &kind) in types.iter().enumerate() {
                let value = if index + 1 == types.len() {
                    Some(core::mem::take(&mut rest))
                } else {
                    take_field(kind, &mut rest)
                };

                let Some(value) = value else {
                    break;
                };

                push_field(&mut fields, kind, value);
            }
        }
        None => {
            while let Some((&kind, tail)) = rest.split_first() {
                rest = tail;

                let Some(value) = take_field(kind, &mut rest) else {
                    break;
                };

                push_field(&mut fields, kind, value);
            }
        }
    }

    fields
}

/// Lowercase types are text ending at a NUL, or at the end of the data;
/// uppercase ones a big-endian size, then that many bytes.
fn take_field<'a>(kind: u8, rest: &mut &'a [u8]) -> Option<&'a [u8]> {
    if kind.is_ascii_lowercase() {
        let end = rest
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(rest.len());
        let value = &rest[..end];

        *rest = rest.get(end + 1..).unwrap_or_default();

        Some(value)
    } else {
        let (size, tail) = rest.split_first_chunk::<4>()?;
        let size = u32::from_be_bytes(*size) as usize;
        let value = tail.get(..size)?;

        *rest = &tail[size..];

        Some(value)
    }
}

fn push_field(fields: &mut Vec<DefinitionField>, kind: u8, value: &[u8]) {
    let Some(kind) = FieldKind::from_type(kind) else {
        return;
    };

    fields.push(DefinitionField {
        kind,
        text: String::from_utf8_lossy(value).to_string(),
    });
}
