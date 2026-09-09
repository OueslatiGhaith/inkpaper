use super::*;

struct TestFile(PathBuf);

impl TestFile {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "inkpaper-progress-{}-{}.epub",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        fs::write(&path, b"book revision one").unwrap();
        Self(path)
    }
}

impl Drop for TestFile {
    fn drop(&mut self) {
        let mut progress = self.0.as_os_str().to_os_string();
        progress.push(".inkpaper-progress");
        let _ = fs::remove_file(progress);
        let _ = fs::remove_file(&self.0);
    }
}

fn position(offset: u64, non_text: u64) -> ReadingPosition {
    ReadingPosition::new(
        BookLocation::new(SpineIndex::new(3), ContentOffset::new(offset)),
        non_text,
    )
}

#[test]
fn progress_survives_reopening_and_reflow_does_not_move_the_saved_anchor() {
    let book = TestFile::new();
    let mut progress = ReaderProgress::open(&book.0).unwrap();
    assert_eq!(progress.load().unwrap(), None);

    progress.start_at(position(0, 0));
    progress.checkpoint(position(100, 2)).unwrap();
    let original = fs::read(progress.path()).unwrap();
    drop(progress);

    let mut reopened = ReaderProgress::open(&book.0).unwrap();
    assert_eq!(reopened.load().unwrap(), Some(position(100, 2)));

    // Reflow puts the saved anchor inside a page that starts earlier.
    reopened.start_at(position(80, 2));
    reopened.checkpoint(position(80, 2)).unwrap();
    reopened.flush().unwrap();
    assert_eq!(fs::read(reopened.path()).unwrap(), original);

    reopened.checkpoint(position(120, 3)).unwrap();
    assert_eq!(reopened.load().unwrap(), Some(position(120, 3)));
    assert_eq!(fs::read(&book.0).unwrap(), b"book revision one");
}

#[test]
fn changed_books_and_invalid_records_are_not_restored() {
    let book = TestFile::new();
    let mut progress = ReaderProgress::open(&book.0).unwrap();
    progress.checkpoint(position(100, 2)).unwrap();
    let original = fs::read(progress.path()).unwrap();

    for length in [0, 8, 60, RECORD_BYTES - 1] {
        fs::write(progress.path(), &original[..length]).unwrap();
        assert!(progress.load().is_err());
    }

    let mut corrupt = original.clone();
    corrupt[44] ^= 1;
    fs::write(progress.path(), &corrupt).unwrap();
    assert!(progress.load().is_err());

    corrupt = original.clone();
    corrupt.push(0);
    fs::write(progress.path(), &corrupt).unwrap();
    assert!(progress.load().is_err());

    fs::write(progress.path(), &original).unwrap();
    fs::write(&book.0, b"book revision two").unwrap();
    let replaced = ReaderProgress::open(&book.0).unwrap();
    assert_eq!(replaced.load().unwrap(), None);
}

#[test]
fn failed_writes_keep_a_pending_checkpoint_for_exit_retry() {
    let book = TestFile::new();
    let mut progress = ReaderProgress::open(&book.0).unwrap();
    progress.checkpoint(position(10, 1)).unwrap();
    let original_path = progress.path.clone();
    let original = fs::read(&original_path).unwrap();

    // a path through a regular file cannot be used as a destination directory.
    progress.path = book.0.join("progress");
    assert!(progress.checkpoint(position(20, 2)).is_err());
    assert_eq!(fs::read(&original_path).unwrap(), original);

    progress.path = original_path;
    progress.flush().unwrap();
    assert_eq!(progress.load().unwrap(), Some(position(20, 2)));
}
