//! Files on the SD card are CBOR written by minicbor, whose fields are known by
//! index number rather than position, so stored types change without a version
//! number. Older files lack fields added later, so a new field must be an
//! `Option` or `#[cbor(default)]`. A removed field's index must never be reused.

use alloc::vec::Vec;

use minicbor::{Decode, Decoder, Encode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StorageError {
    Encode,
    Decode,
    TrailingData,
}

pub(crate) fn encode<T: Encode<()>>(value: &T) -> Result<Vec<u8>, StorageError> {
    minicbor::to_vec(value).map_err(|_| StorageError::Encode)
}

pub(crate) fn decode<'b, T: Decode<'b, ()>>(bytes: &'b [u8]) -> Result<T, StorageError> {
    let mut decoder = Decoder::new(bytes);
    let value = decoder.decode().map_err(|_| StorageError::Decode)?;

    if decoder.position() != bytes.len() {
        return Err(StorageError::TrailingData);
    }

    Ok(value)
}
