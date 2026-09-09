use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use inkpaper_epub::{BookLocation, ContentOffset, SpineIndex};
use inkpaper_reader::ReadingPosition;
use sha2::{Digest, Sha256};

#[cfg(test)]
mod tests;

// version both the record layout and the normalized-flow position semantics.
const MAGIC: &[u8; 8] = b"INKPOS01";
const RECORD_BYTES: usize = 92;

pub struct ReaderProgress {
    path: PathBuf,
    book_hash: [u8; 32],
    visible: Option<ReadingPosition>,
    pending: Option<ReadingPosition>,
}

impl ReaderProgress {
    pub fn open(book: &Path) -> io::Result<Self> {
        let mut file = File::open(book)?;
        let mut hash = Sha256::new();
        let mut buffer = [0u8; 8192];

        loop {
            let count = match file.read(&mut buffer) {
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                result => result?,
            };
            if count == 0 {
                break;
            }
            hash.update(&buffer[..count]);
        }

        let mut path = book.as_os_str().to_os_string();
        path.push(".inkpaper-progress");

        Ok(Self {
            path: PathBuf::from(path),
            book_hash: hash.finalize().into(),
            visible: None,
            pending: None,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> io::Result<Option<ReadingPosition>> {
        let mut file = match File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };

        let mut record = [0u8; RECORD_BYTES];
        file.read_exact(&mut record)?;
        let mut extra = [0u8; 1];
        let checksum = Sha256::digest(&record[..60]);

        if file.read(&mut extra)? != 0 || &record[..8] != MAGIC || record[60..] != checksum[..] {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid reader progress record",
            ));
        }

        if record[8..40] != self.book_hash {
            return Ok(None);
        }

        let spine = u32::from_le_bytes(record[40..44].try_into().unwrap());
        let offset = u64::from_le_bytes(record[44..52].try_into().unwrap());
        let non_text = u64::from_le_bytes(record[52..60].try_into().unwrap());

        Ok(Some(ReadingPosition::new(
            BookLocation::new(SpineIndex::new(spine), ContentOffset::new(offset)),
            non_text,
        )))
    }

    /// establish the displayed page without replacing a restored anchor after reflow.
    pub fn start_at(&mut self, position: ReadingPosition) {
        self.visible = Some(position);
        self.pending = None;
    }

    /// only a changed displayed page creates a new checkpoint.
    pub fn checkpoint(&mut self, position: ReadingPosition) -> io::Result<()> {
        if self.visible == Some(position) {
            return Ok(());
        }

        self.visible = Some(position);
        self.pending = Some(position);
        self.flush()
    }

    /// retry an unsuccessful write when the simulator exits.
    pub fn flush(&mut self) -> io::Result<()> {
        let Some(position) = self.pending else {
            return Ok(());
        };

        let mut record = [0u8; RECORD_BYTES];
        record[..8].copy_from_slice(MAGIC);
        record[8..40].copy_from_slice(&self.book_hash);
        record[40..44].copy_from_slice(&position.location().spine().get().to_le_bytes());
        record[44..52].copy_from_slice(&position.location().offset().get().to_le_bytes());
        record[52..60].copy_from_slice(&position.non_text().to_le_bytes());
        let checksum = Sha256::digest(&record[..60]);
        record[60..].copy_from_slice(&checksum);

        write_progress(&self.path, &record)?;
        self.pending = None;
        Ok(())
    }
}

fn write_progress(path: &Path, record: &[u8]) -> io::Result<()> {
    static NEXT: AtomicU64 = AtomicU64::new(0);

    let (temporary, mut file) = loop {
        let mut temporary = path.as_os_str().to_os_string();
        temporary.push(format!(
            ".{}.{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let temporary = PathBuf::from(temporary);

        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => break (temporary, file),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    };

    let result = (|| {
        file.write_all(record)?;
        file.sync_all()
    })();

    drop(file);
    let result = result.and_then(|()| fs::rename(&temporary, path));

    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }

    result
}
