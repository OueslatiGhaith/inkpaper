mod support;

use futures_lite::future;
use inkpaper_dictionary::{Definition, Dictionary, DictionaryInfo, Error};
use inkpaper_epub::{SliceSource, SliceSourceError};

use support::{GzipHeader, StarDict, dictzip, stardict};

fn open<'a>(
    dictionary: &'a StarDict,
    data: &'a [u8],
) -> Result<Dictionary<SliceSource<'a>>, Error<SliceSourceError>> {
    let info = DictionaryInfo::parse(dictionary.ifo.as_bytes()).unwrap();

    future::block_on(Dictionary::open(
        info,
        SliceSource::new(&dictionary.idx),
        SliceSource::new(data),
    ))
}

fn lookup(dictionary: &mut Dictionary<SliceSource<'_>>, word: &str) -> Option<Definition> {
    future::block_on(dictionary.lookup(word)).unwrap()
}

fn sentences(count: usize) -> (Vec<String>, Vec<String>) {
    let words = (0..count).map(|index| format!("word{index:04}")).collect();
    let definitions = (0..count)
        .map(|index| {
            format!(
                "definition number {index}, {}",
                "and so on ".repeat(index % 7)
            )
        })
        .collect();

    (words, definitions)
}

#[test]
fn reads_the_same_definitions_as_the_uncompressed_dict() {
    let (words, definitions) = sentences(300);
    let entries: Vec<(&str, &[u8])> = words
        .iter()
        .zip(&definitions)
        .map(|(word, definition)| (word.as_str(), definition.as_bytes()))
        .collect();

    let dictionary = stardict(Some("m"), &entries);
    let compressed = dictzip(&dictionary.dict, 1_000, &GzipHeader::default());

    assert!(dictionary.dict.len() > 10_000);

    let mut plain = open(&dictionary, &dictionary.dict).unwrap();
    let mut packed = open(&dictionary, &compressed).unwrap();

    for word in &words {
        assert_eq!(lookup(&mut packed, word), lookup(&mut plain, word));
    }
}

#[test]
fn inflates_a_definition_across_several_chunks() {
    let long = "a long definition ".repeat(150);
    let dictionary = stardict(
        Some("m"),
        &[("aa", b"first"), ("long", long.as_bytes()), ("zz", b"last")],
    );
    let compressed = dictzip(&dictionary.dict, 1_000, &GzipHeader::default());

    let mut opened = open(&dictionary, &compressed).unwrap();

    assert!(long.len() > 2_000);
    assert_eq!(
        lookup(&mut opened, "long").unwrap().fields()[0].text(),
        long
    );
    assert_eq!(
        lookup(&mut opened, "zz").unwrap().fields()[0].text(),
        "last"
    );
}

#[test]
fn skips_the_optional_header_fields() {
    let dictionary = stardict(
        Some("m"),
        &[("cat", b"a small feline"), ("dog", b"a canine")],
    );
    let header = GzipHeader {
        name: Some("test.dict"),
        comment: Some("built for a test"),
        crc: true,
    };
    let compressed = dictzip(&dictionary.dict, 16, &header);

    let mut opened = open(&dictionary, &compressed).unwrap();

    assert_eq!(
        lookup(&mut opened, "dog").unwrap().fields()[0].text(),
        "a canine"
    );
}

#[test]
fn rejects_gzip_without_a_chunk_table() {
    let dictionary = stardict(Some("m"), &[("cat", b"a small feline")]);

    let mut plain_gzip = vec![0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 2, 3];
    plain_gzip.extend_from_slice(&miniz_oxide::deflate::compress_to_vec(&dictionary.dict, 6));
    plain_gzip.extend_from_slice(&0u32.to_le_bytes());
    plain_gzip.extend_from_slice(&(dictionary.dict.len() as u32).to_le_bytes());

    assert!(matches!(
        open(&dictionary, &plain_gzip),
        Err(Error::MalformedDictZip),
    ));
}

#[test]
fn reports_a_damaged_chunk() {
    let (words, definitions) = sentences(100);
    let entries: Vec<(&str, &[u8])> = words
        .iter()
        .zip(&definitions)
        .map(|(word, definition)| (word.as_str(), definition.as_bytes()))
        .collect();

    let dictionary = stardict(Some("m"), &entries);
    let mut compressed = dictzip(&dictionary.dict, 1_000, &GzipHeader::default());

    // the first chunk starts after the header and its RA field
    let chunk_count = dictionary.dict.len().div_ceil(1_000);
    let first_chunk = 12 + 4 + 6 + chunk_count * 2;

    // a final block of the reserved type
    compressed[first_chunk] = 0xff;

    let mut opened = open(&dictionary, &compressed).unwrap();

    assert!(matches!(
        future::block_on(opened.lookup("word0000")),
        Err(Error::Inflate),
    ));
}
