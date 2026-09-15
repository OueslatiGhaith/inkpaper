use std::{
    fs::File,
    io::{self, Read, Seek, SeekFrom},
    path::Path,
};

use inkpaper_epub::EpubSource;

#[derive(Debug)]
pub struct HostFileSource {
    file: File,
    len: u64,
}

impl HostFileSource {
    pub fn open(path: &Path) -> io::Result<Self> {
        let file = File::open(path)?;
        let len = file.metadata()?.len();

        Ok(Self { file, len })
    }
}

impl EpubSource for HostFileSource {
    type Error = io::Error;

    async fn len(&mut self) -> Result<u64, Self::Error> {
        Ok(self.len)
    }

    async fn read_exact_at(&mut self, offset: u64, buffer: &mut [u8]) -> Result<(), Self::Error> {
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.read_exact(buffer)
    }
}
