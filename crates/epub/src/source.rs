pub trait EpubSource {
    type Error;

    async fn len(&mut self) -> Result<u64, Self::Error>;

    async fn read_exact_at(&mut self, offset: u64, buffer: &mut [u8]) -> Result<(), Self::Error>;
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SliceSourceError {
    OutOfBounds,
}

impl EpubSource for SliceSource<'_> {
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
