//! Offline lookups in StarDict dictionaries, read from the card a piece at a
//! time. Follows crosspoint's reader: an uncompressed `.idx`, a `.dict` or
//! `.dict.dz`, and the `.ifo` describing them.

#![no_std]

extern crate alloc;

mod definition;
mod dictionary;
mod dictzip;
mod error;
mod forms;
mod index;
mod info;
mod samples;

pub use definition::{Definition, DefinitionField, FieldKind};
pub use dictionary::Dictionary;
pub use error::{Error, InfoError};
pub use info::DictionaryInfo;
pub use samples::IndexSamples;
