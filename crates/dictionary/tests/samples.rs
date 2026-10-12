mod support;

use std::cell::Cell;

use futures_lite::future;
use inkpaper_dictionary::{Dictionary, DictionaryInfo, IndexSamples};
use inkpaper_epub::{EpubSource, SliceSource, SliceSourceError};

use support::{StarDict, stardict};

/// A source counting the bytes read from it.
struct CountingSource<'a> {
    source: SliceSource<'a>,
    read: &'a Cell<usize>,
}

impl EpubSource for CountingSource<'_> {
    type Error = SliceSourceError;

    async fn len(&mut self) -> Result<u64, Self::Error> {
        self.source.len().await
    }

    async fn read_exact_at(&mut self, offset: u64, buffer: &mut [u8]) -> Result<(), Self::Error> {
        self.read.set(self.read.get() + buffer.len());

        self.source.read_exact_at(offset, buffer).await
    }
}

/// Opens `dictionary`, counting the bytes read from both its files in `read`.
fn open<'a>(dictionary: &'a StarDict, read: &'a Cell<usize>) -> Dictionary<CountingSource<'a>> {
    let info = DictionaryInfo::parse(dictionary.ifo.as_bytes()).unwrap();

    future::block_on(Dictionary::open(
        info,
        CountingSource {
            source: SliceSource::new(&dictionary.idx),
            read,
        },
        CountingSource {
            source: SliceSource::new(&dictionary.dict),
            read,
        },
    ))
    .unwrap()
}

fn numbered(count: usize) -> StarDict {
    let words: Vec<String> = (0..count).map(|index| format!("word{index:05}")).collect();
    let entries: Vec<(&str, &[u8])> = words
        .iter()
        .map(|word| (word.as_str(), word.as_bytes()))
        .collect();

    stardict(Some("m"), &entries)
}

fn headword(dictionary: &mut Dictionary<CountingSource<'_>>, word: &str) -> Option<String> {
    future::block_on(dictionary.lookup(word))
        .unwrap()
        .map(|definition| definition.headword().to_string())
}

fn encoded(index_len: usize, offsets: &[usize]) -> Vec<u8> {
    let mut bytes = b"IXS1".to_vec();
    bytes.extend_from_slice(&(index_len as u64).to_le_bytes());
    bytes.extend_from_slice(&(offsets.len() as u32).to_le_bytes());

    for &offset in offsets {
        bytes.extend_from_slice(&(offset as u32).to_le_bytes());
    }

    bytes
}

#[test]
fn samples_every_256th_entry() {
    let dictionary = numbered(600);
    let read = Cell::new(0);
    let mut opened = open(&dictionary, &read);

    let samples = future::block_on(opened.sample_index()).unwrap().encode();

    // "wordNNNNN\0" and its offset and size
    let entry_len = 18;

    assert_eq!(
        samples,
        encoded(dictionary.idx.len(), &[0, 256 * entry_len, 512 * entry_len]),
    );
}

#[test]
fn finds_words_around_the_samples() {
    let dictionary = numbered(2_000);
    let read = Cell::new(0);
    let mut opened = open(&dictionary, &read);

    future::block_on(opened.sample_index()).unwrap();

    for index in [0, 1, 255, 256, 257, 511, 512, 1_792, 1_999] {
        let word = format!("word{index:05}");

        assert_eq!(headword(&mut opened, &word), Some(word));
    }

    assert_eq!(headword(&mut opened, "aardvark"), None);
    assert_eq!(headword(&mut opened, "word00256x"), None);
    assert_eq!(headword(&mut opened, "zebra"), None);
}

#[test]
fn reads_a_fraction_of_the_files_through_samples() {
    let dictionary = numbered(20_000);
    let read = Cell::new(0);
    let mut opened = open(&dictionary, &read);

    future::block_on(opened.sample_index()).unwrap();
    read.set(0);

    assert!(headword(&mut opened, "word19999").is_some());
    assert!(
        read.get() < dictionary.idx.len() / 20,
        "read {} bytes",
        read.get(),
    );
}

#[test]
fn finds_a_case_run_split_by_a_sample() {
    let fillers: Vec<String> = (0..255).map(|index| format!("a{index:03}")).collect();

    let mut entries: Vec<(&str, &[u8])> = fillers
        .iter()
        .map(|word| (word.as_str(), &b"filler"[..]))
        .collect();
    entries.push(("Polish", b"from Poland"));
    entries.push(("polish", b"to make shine"));

    let dictionary = stardict(Some("m"), &entries);
    let read = Cell::new(0);
    let mut opened = open(&dictionary, &read);

    future::block_on(opened.sample_index()).unwrap();

    assert_eq!(headword(&mut opened, "POLISH").as_deref(), Some("Polish"));
    assert_eq!(headword(&mut opened, "polish").as_deref(), Some("polish"));
}

#[test]
fn round_trips_saved_samples() {
    let dictionary = numbered(2_000);
    let read = Cell::new(0);

    let saved = future::block_on(open(&dictionary, &read).sample_index())
        .unwrap()
        .encode();

    let mut reopened = open(&dictionary, &read);

    assert!(reopened.use_samples(IndexSamples::decode(&saved).unwrap()));
    assert_eq!(
        headword(&mut reopened, "word01500").as_deref(),
        Some("word01500"),
    );
}

#[test]
fn refuses_samples_of_another_index() {
    let dictionary = numbered(2_000);
    let other = numbered(1_000);
    let read = Cell::new(0);

    let samples = future::block_on(open(&other, &read).sample_index())
        .unwrap()
        .encode();

    let mut opened = open(&dictionary, &read);

    assert!(!opened.use_samples(IndexSamples::decode(&samples).unwrap()));
}

#[test]
fn rejects_damaged_saved_samples() {
    let saved = encoded(10_000, &[0, 4_608, 9_216]);

    assert!(IndexSamples::decode(&saved).is_some());
    assert_eq!(IndexSamples::decode(&saved[..saved.len() - 1]), None);
    assert_eq!(IndexSamples::decode(b"IXS0"), None);
    assert_eq!(
        IndexSamples::decode(&encoded(10_000, &[0, 9_216, 4_608])),
        None,
    );
    assert_eq!(IndexSamples::decode(&encoded(10_000, &[18, 4_608])), None);
    assert_eq!(IndexSamples::decode(&encoded(10_000, &[0, 10_000])), None);
}
