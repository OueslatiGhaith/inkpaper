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

#[cfg(test)]
mod tests {
    use std::{fs, process};

    use futures_lite::future;

    use super::*;

    #[test]
    fn host_file_source_reads_random_ranges() {
        let path =
            std::env::temp_dir().join(format!("inkpaper-host-epub-source-{}.bin", process::id()));

        fs::write(&path, b"0123456789").unwrap();

        let mut source = HostFileSource::open(&path).unwrap();

        assert_eq!(future::block_on(source.len()).unwrap(), 10);

        let mut buffer = [0u8; 4];

        future::block_on(source.read_exact_at(3, &mut buffer)).unwrap();

        assert_eq!(&buffer, b"3456");

        drop(source);
        fs::remove_file(path).unwrap();
    }
}
