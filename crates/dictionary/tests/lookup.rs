mod support;

use futures_lite::future;
use inkpaper_dictionary::{Definition, Dictionary, DictionaryInfo, Error, FieldKind};
use inkpaper_epub::SliceSource;

use support::{StarDict, stardict};

fn lookup(dictionary: &StarDict, word: &str) -> Option<Definition> {
    let info = DictionaryInfo::parse(dictionary.ifo.as_bytes()).unwrap();

    let mut dictionary = future::block_on(Dictionary::open(
        info,
        SliceSource::new(&dictionary.idx),
        SliceSource::new(&dictionary.dict),
    ))
    .unwrap();

    future::block_on(dictionary.lookup(word)).unwrap()
}

fn fields(definition: &Definition) -> Vec<(FieldKind, &str)> {
    definition
        .fields()
        .iter()
        .map(|field| (field.kind(), field.text()))
        .collect()
}

#[test]
fn finds_a_word_and_its_definition() {
    let dictionary = stardict(
        Some("m"),
        &[
            ("apple", b"a round fruit"),
            ("book", b"a written work"),
            ("cat", b"a small feline"),
        ],
    );

    let definition = lookup(&dictionary, "book").unwrap();

    assert_eq!(definition.headword(), "book");
    assert_eq!(fields(&definition), [(FieldKind::Plain, "a written work")]);
}

#[test]
fn misses_words_before_between_and_after_the_entries() {
    let dictionary = stardict(
        Some("m"),
        &[("book", b"a written work"), ("dog", b"a canine")],
    );

    assert_eq!(lookup(&dictionary, "apple"), None);
    assert_eq!(lookup(&dictionary, "cat"), None);
    assert_eq!(lookup(&dictionary, "zebra"), None);
    assert_eq!(lookup(&dictionary, "boo"), None);
}

#[test]
fn matches_ascii_letters_in_any_case() {
    let dictionary = stardict(Some("m"), &[("London", b"a city"), ("lone", b"single")]);

    let definition = lookup(&dictionary, "LONDON").unwrap();

    assert_eq!(definition.headword(), "London");
}

#[test]
fn prefers_the_entry_in_the_word_s_own_case() {
    let dictionary = stardict(
        Some("m"),
        &[("Polish", b"from Poland"), ("polish", b"to make shine")],
    );

    assert_eq!(lookup(&dictionary, "polish").unwrap().headword(), "polish");
    assert_eq!(lookup(&dictionary, "Polish").unwrap().headword(), "Polish");
    assert_eq!(lookup(&dictionary, "POLISH").unwrap().headword(), "Polish");
}

#[test]
fn finds_words_past_the_first_buffer() {
    let words: Vec<String> = (0..2_000).map(|index| format!("word{index:05}")).collect();
    let entries: Vec<(&str, &[u8])> = words
        .iter()
        .map(|word| (word.as_str(), word.as_bytes()))
        .collect();

    let dictionary = stardict(Some("m"), &entries);

    assert!(dictionary.idx.len() > 4 * 4096);

    for word in ["word00000", "word00293", "word01024", "word01999"] {
        let definition = lookup(&dictionary, word).unwrap();

        assert_eq!(fields(&definition), [(FieldKind::Plain, word)]);
    }
}

#[test]
fn reads_the_longest_words_across_buffers() {
    let long = "x".repeat(255);
    let words: Vec<String> = (0..40).map(|index| format!("{index:02}{long}")).collect();
    let words: Vec<&str> = words.iter().map(|word| &word[..255]).collect();
    let entries: Vec<(&str, &[u8])> = words.iter().map(|&word| (word, &b"long"[..])).collect();

    let dictionary = stardict(Some("m"), &entries);

    for word in [words[0], words[15], words[39]] {
        assert_eq!(lookup(&dictionary, word).unwrap().headword(), word);
    }
}

#[test]
fn splits_a_shared_type_sequence() {
    let dictionary = stardict(Some("tm"), &[("cat", b"k\xc3\xa6t\0a small feline")]);

    let definition = lookup(&dictionary, "cat").unwrap();

    assert_eq!(
        fields(&definition),
        [
            (FieldKind::Phonetic, "kæt"),
            (FieldKind::Plain, "a small feline"),
        ],
    );
}

#[test]
fn splits_typed_fields_and_skips_binary_ones() {
    let mut data = Vec::new();
    data.extend_from_slice(b"h<b>cat</b>\0");
    data.extend_from_slice(b"P");
    data.extend_from_slice(&3u32.to_be_bytes());
    data.extend_from_slice(b"\x89\0\x01");
    data.extend_from_slice(b"ma small feline\0");

    let dictionary = stardict(None, &[("cat", &data)]);

    let definition = lookup(&dictionary, "cat").unwrap();

    assert_eq!(
        fields(&definition),
        [
            (FieldKind::Html, "<b>cat</b>"),
            (FieldKind::Plain, "a small feline"),
        ],
    );
}

#[test]
fn keeps_the_fields_of_cut_short_data() {
    let mut data = b"ma small feline\0W".to_vec();
    data.extend_from_slice(&100u32.to_be_bytes());
    data.extend_from_slice(b"RIFF");

    let dictionary = stardict(None, &[("cat", &data)]);

    let definition = lookup(&dictionary, "cat").unwrap();

    assert_eq!(fields(&definition), [(FieldKind::Plain, "a small feline")]);
}

#[test]
fn reports_a_truncated_index() {
    let mut dictionary = stardict(Some("m"), &[("apple", b"fruit"), ("book", b"work")]);
    dictionary.idx.truncate(dictionary.idx.len() - 3);

    let info = DictionaryInfo::parse(dictionary.ifo.as_bytes()).unwrap();
    let mut opened = future::block_on(Dictionary::open(
        info,
        SliceSource::new(&dictionary.idx),
        SliceSource::new(&dictionary.dict),
    ))
    .unwrap();

    assert!(matches!(
        future::block_on(opened.lookup("book")),
        Err(Error::MalformedIndex),
    ));
}

#[test]
fn reports_a_definition_past_the_data() {
    let mut dictionary = stardict(Some("m"), &[("apple", b"fruit")]);
    dictionary.dict.truncate(2);

    let info = DictionaryInfo::parse(dictionary.ifo.as_bytes()).unwrap();
    let mut opened = future::block_on(Dictionary::open(
        info,
        SliceSource::new(&dictionary.idx),
        SliceSource::new(&dictionary.dict),
    ))
    .unwrap();

    assert!(matches!(
        future::block_on(opened.lookup("apple")),
        Err(Error::DefinitionOutOfBounds),
    ));
}
