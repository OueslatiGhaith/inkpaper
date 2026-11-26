//! Random-access byte source abstractions shared across `no_std` storage readers.

#![no_std]

use core::fmt;

/// Asynchronous random-access read-only byte source.
#[allow(async_fn_in_trait)]
#[allow(clippy::len_without_is_empty)]
pub trait ByteSource {
    type Error;

    async fn len(&mut self) -> Result<u64, Self::Error>;

    async fn read_exact_at(&mut self, offset: u64, buffer: &mut [u8]) -> Result<(), Self::Error>;
}

impl<T: ByteSource + ?Sized> ByteSource for &mut T {
    type Error = T::Error;

    async fn len(&mut self) -> Result<u64, Self::Error> {
        (**self).len().await
    }

    async fn read_exact_at(&mut self, offset: u64, buffer: &mut [u8]) -> Result<(), Self::Error> {
        (**self).read_exact_at(offset, buffer).await
    }
}

/// In-memory [`ByteSource`] backed by a byte slice.
#[derive(Debug, Clone, Copy)]
pub struct SliceSource<'a> {
    bytes: &'a [u8],
}

impl<'a> SliceSource<'a> {
    pub const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }

    pub const fn bytes(self) -> &'a [u8] {
        self.bytes
    }
}

/// Error returned by [`SliceSource`] when a read exceeds the underlying slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SliceSourceError {
    OutOfBounds,
}

impl fmt::Display for SliceSourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfBounds => f.write_str("read out of bounds"),
        }
    }
}

impl core::error::Error for SliceSourceError {}

impl ByteSource for SliceSource<'_> {
    type Error = SliceSourceError;

    async fn len(&mut self) -> Result<u64, Self::Error> {
        u64::try_from(self.bytes.len()).map_err(|_| SliceSourceError::OutOfBounds)
    }

    async fn read_exact_at(&mut self, offset: u64, buffer: &mut [u8]) -> Result<(), Self::Error> {
        let start = usize::try_from(offset).map_err(|_| SliceSourceError::OutOfBounds)?;
        let end = start
            .checked_add(buffer.len())
            .ok_or(SliceSourceError::OutOfBounds)?;

        let source = self
            .bytes
            .get(start..end)
            .ok_or(SliceSourceError::OutOfBounds)?;

        buffer.copy_from_slice(source);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use futures_lite::future;

    use super::{ByteSource, SliceSource, SliceSourceError};

    #[test]
    fn reads_length_and_slices() {
        let mut source = SliceSource::new(b"inkpaper");

        assert_eq!(source.bytes(), b"inkpaper");
        assert_eq!(future::block_on(source.len()), Ok(8));

        let mut buf = [0u8; 5];
        assert_eq!(future::block_on(source.read_exact_at(3, &mut buf)), Ok(()));
        assert_eq!(&buf, b"paper");

        let mut empty = [];
        assert_eq!(
            future::block_on(source.read_exact_at(8, &mut empty)),
            Ok(())
        );
    }

    #[test]
    fn rejects_out_of_bounds_and_overflow() {
        let mut source = SliceSource::new(b"abc");
        let mut buf = [0u8; 2];

        assert_eq!(
            future::block_on(source.read_exact_at(2, &mut buf)),
            Err(SliceSourceError::OutOfBounds)
        );
        assert_eq!(
            future::block_on(source.read_exact_at(u64::MAX, &mut buf)),
            Err(SliceSourceError::OutOfBounds)
        );
        let mut empty = [];
        assert_eq!(
            future::block_on(source.read_exact_at(4, &mut empty)),
            Err(SliceSourceError::OutOfBounds)
        );
    }

    #[test]
    fn mutable_reference_forwards_byte_source() {
        async fn read_two<S: ByteSource>(mut source: S) -> Result<(u64, [u8; 2]), S::Error> {
            let len = source.len().await?;
            let mut buf = [0u8; 2];
            source.read_exact_at(1, &mut buf).await?;
            Ok((len, buf))
        }

        let mut source = SliceSource::new(b"xyz");
        assert_eq!(future::block_on(read_two(&mut source)), Ok((3, *b"yz")));
    }
}
