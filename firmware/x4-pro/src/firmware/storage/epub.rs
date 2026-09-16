use alloc::string::String;
use defmt::{debug, info, warn};
use hadris_fat::r#async::FatVolume;
use hadris_io::{
    SeekFrom,
    r#async::{Read as HadrisRead, Seek as HadrisSeek},
};
use inkpaper_app::ReaderSession;
use inkpaper_epub::EpubSource;

#[derive(Debug)]
pub enum FatEpubSourceError {
    Storage(hadris_fat::Error),
    UnexpectedEof,
}

pub(super) struct FatEpubSource<'a, D>
where
    D: HadrisRead + HadrisSeek<Error = <D as HadrisRead>::Error>,
{
    reader: hadris_fat::r#async::read::FileReader<'a, D>,
    len: u64,
}

impl<'a, D> FatEpubSource<'a, D>
where
    D: HadrisRead + HadrisSeek<Error = <D as HadrisRead>::Error>,
{
    fn new(reader: hadris_fat::r#async::read::FileReader<'a, D>) -> Self {
        let len = reader.size() as u64;

        Self { reader, len }
    }
}

impl<D> EpubSource for FatEpubSource<'_, D>
where
    D: HadrisRead + HadrisSeek<Error = <D as HadrisRead>::Error>,
{
    type Error = FatEpubSourceError;

    async fn len(&mut self) -> Result<u64, Self::Error> {
        Ok(self.len)
    }

    async fn read_exact_at(&mut self, offset: u64, buffer: &mut [u8]) -> Result<(), Self::Error> {
        self.reader
            .seek(SeekFrom::Start(offset))
            .await
            .map_err(FatEpubSourceError::Storage)?;

        let mut read = 0usize;

        while read < buffer.len() {
            let count = self
                .reader
                .read(&mut buffer[read..])
                .await
                .map_err(FatEpubSourceError::Storage)?;

            if count == 0 {
                return Err(FatEpubSourceError::UnexpectedEof);
            }

            read += count;
        }

        Ok(())
    }
}

pub(super) async fn open_reader_session<'a, D>(
    filesystem: &'a FatVolume<D>,
    path: String,
) -> Option<ReaderSession<FatEpubSource<'a, D>>>
where
    D: HadrisRead + HadrisSeek<Error = <D as HadrisRead>::Error>,
{
    debug!("opening EPUB session path={}", path.as_str());

    let reader = match filesystem.open_file_path(&path).await {
        Ok(reader) => reader,
        Err(error) => {
            warn!(
                "EPUB file open failed path={} error={:?}",
                path.as_str(),
                error,
            );

            return None;
        }
    };

    let source = FatEpubSource::new(reader);

    match ReaderSession::open(path.clone(), source).await {
        Ok(session) => {
            info!("EPUB session opened path={}", path.as_str());

            Some(session)
        }
        Err(_) => {
            warn!("EPUB session parse failed path={}", path.as_str());

            None
        }
    }
}
