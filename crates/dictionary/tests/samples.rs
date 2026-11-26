mod support;

use std::cell::Cell;

use futures_lite::future;
use inkpaper_dictionary::{Dictionary, DictionaryInfo, IndexSamples};
use inkpaper_io::{ByteSource, SliceSource, SliceSourceError};

use support::{StarDict, stardict};

/// A source counting the bytes and calls read from it.
struct CountingSource<'a> {
    source: SliceSource<'a>,
    bytes: &'a Cell<usize>,
    calls: &'a Cell<usize>,
}

impl ByteSource for CountingSource<'_> {
    type Error = SliceSourceError;

    async fn len(&mut self) -> Result<u64, Self::Error> {
        self.source.len().await
    }

    async fn read_exact_at(&mut self, offset: u64, buffer: &mut [u8]) -> Result<(), Self::Error> {
        self.bytes.set(self.bytes.get() + buffer.len());
        self.calls.set(self.calls.get() + 1);

        self.source.read_exact_at(offset, buffer).await
    }
}

/// Opens `dictionary`, counting reads from its index and data files separately.
fn open_counted<'a>(
    dictionary: &'a StarDict,
    idx_bytes: &'a Cell<usize>,
    idx_calls: &'a Cell<usize>,
    dict_bytes: &'a Cell<usize>,
    dict_calls: &'a Cell<usize>,
) -> Dictionary<CountingSource<'a>> {
    let info = DictionaryInfo::parse(dictionary.ifo.as_bytes()).unwrap();

    future::block_on(Dictionary::open(
        info,
        CountingSource {
            source: SliceSource::new(&dictionary.idx),
            bytes: idx_bytes,
            calls: idx_calls,
        },
        CountingSource {
            source: SliceSource::new(&dictionary.dict),
            bytes: dict_bytes,
            calls: dict_calls,
        },
    ))
    .unwrap()
}

/// Opens `dictionary`, counting the bytes read from both its files in `read`.
fn open<'a>(
    dictionary: &'a StarDict,
    read: &'a Cell<usize>,
    calls: &'a Cell<usize>,
) -> Dictionary<CountingSource<'a>> {
    open_counted(dictionary, read, calls, read, calls)
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

fn encoded(index_len: usize, samples: &[(usize, &[u8])]) -> Vec<u8> {
    let mut bytes = b"IXS2".to_vec();
    bytes.extend_from_slice(&(index_len as u64).to_le_bytes());
    bytes.extend_from_slice(&(samples.len() as u32).to_le_bytes());

    for &(offset, word) in samples {
        bytes.extend_from_slice(&(offset as u32).to_le_bytes());
        bytes.push(word.len() as u8);
        bytes.extend_from_slice(word);
    }

    // without synonyms
    bytes.push(0);

    bytes
}

#[test]
fn samples_every_256th_entry() {
    let dictionary = numbered(600);
    let read = Cell::new(0);
    let calls = Cell::new(0);
    let mut opened = open(&dictionary, &read, &calls);

    let samples = future::block_on(opened.sample_index()).unwrap().encode();

    // "wordNNNNN\0" and its offset and size
    let entry_len = 18;

    assert_eq!(
        samples,
        encoded(
            dictionary.idx.len(),
            &[
                (0, b"word00000"),
                (256 * entry_len, b"word00256"),
                (512 * entry_len, b"word00512"),
            ],
        ),
    );
}

#[test]
fn finds_words_around_the_samples() {
    let dictionary = numbered(2_000);
    let read = Cell::new(0);
    let calls = Cell::new(0);
    let mut opened = open(&dictionary, &read, &calls);

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
fn binary_search_does_zero_index_reads() {
    let dictionary = numbered(20_000);
    let idx_bytes = Cell::new(0);
    let idx_calls = Cell::new(0);
    let dict_bytes = Cell::new(0);
    let dict_calls = Cell::new(0);
    let mut opened = open_counted(
        &dictionary,
        &idx_bytes,
        &idx_calls,
        &dict_bytes,
        &dict_calls,
    );

    future::block_on(opened.sample_index()).unwrap();
    idx_bytes.set(0);
    idx_calls.set(0);
    dict_bytes.set(0);
    dict_calls.set(0);

    // Entry 19999 sits 31 entries after the last sample (entry 19968 = 78 * 256),
    // so only a single 32-entry (576-byte) sequential read should touch `.idx`.
    assert_eq!(
        headword(&mut opened, "word19999").as_deref(),
        Some("word19999"),
    );
    assert_eq!(idx_calls.get(), 1);
    assert_eq!(idx_bytes.get(), 32 * 18);
    assert_eq!(dict_calls.get(), 1);
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
    let calls = Cell::new(0);
    let mut opened = open(&dictionary, &read, &calls);

    future::block_on(opened.sample_index()).unwrap();

    assert_eq!(headword(&mut opened, "POLISH").as_deref(), Some("Polish"));
    assert_eq!(headword(&mut opened, "polish").as_deref(), Some("polish"));
}

#[test]
fn round_trips_saved_samples() {
    let dictionary = numbered(2_000);
    let read = Cell::new(0);
    let calls = Cell::new(0);

    let saved = future::block_on(open(&dictionary, &read, &calls).sample_index())
        .unwrap()
        .encode();

    let mut reopened = open(&dictionary, &read, &calls);

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
    let calls = Cell::new(0);

    let samples = future::block_on(open(&other, &read, &calls).sample_index())
        .unwrap()
        .encode();

    let mut opened = open(&dictionary, &read, &calls);

    assert!(!opened.use_samples(IndexSamples::decode(&samples).unwrap()));
}

#[test]
fn rejects_damaged_saved_samples() {
    let saved = encoded(
        10_000,
        &[(0, b"apple"), (4_608, b"mango"), (9_216, b"zebra")],
    );

    assert!(IndexSamples::decode(&saved).is_some());
    assert_eq!(IndexSamples::decode(&saved[..saved.len() - 1]), None);
    assert_eq!(IndexSamples::decode(b"IXS1"), None);
    assert_eq!(IndexSamples::decode(b"IXS0"), None);
    assert_eq!(
        IndexSamples::decode(&encoded(
            10_000,
            &[(0, b"apple"), (9_216, b"mango"), (4_608, b"zebra")],
        )),
        None,
    );
    assert_eq!(
        IndexSamples::decode(&encoded(10_000, &[(18, b"apple"), (4_608, b"mango")])),
        None,
    );
    assert_eq!(
        IndexSamples::decode(&encoded(10_000, &[(0, b"apple"), (10_000, b"mango")])),
        None,
    );
    assert_eq!(
        IndexSamples::decode(&encoded(
            10_000,
            &[(0, b"mango"), (4_608, b"apple")],
        )),
        None,
    );
    assert_eq!(
        IndexSamples::decode(&encoded(10_000, &[(0, b"ap\0ple")])),
        None,
    );
}
