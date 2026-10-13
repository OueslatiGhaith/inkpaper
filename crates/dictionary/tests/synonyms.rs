mod support;

use futures_lite::future;
use inkpaper_dictionary::{Dictionary, DictionaryInfo, Error, IndexSamples};
use inkpaper_epub::{SliceSource, SliceSourceError};

use support::{StarDict, stardict, synonyms};

fn open<'a>(dictionary: &'a StarDict, syn: Option<&'a [u8]>) -> Dictionary<SliceSource<'a>> {
    let info = DictionaryInfo::parse(dictionary.ifo.as_bytes()).unwrap();

    let mut opened = future::block_on(Dictionary::open(
        info,
        SliceSource::new(&dictionary.idx),
        SliceSource::new(&dictionary.dict),
    ))
    .unwrap();

    if let Some(syn) = syn {
        future::block_on(opened.use_synonyms(SliceSource::new(syn))).unwrap();
    }

    opened
}

fn headword(dictionary: &mut Dictionary<SliceSource<'_>>, word: &str) -> Option<String> {
    future::block_on(dictionary.lookup(word))
        .unwrap()
        .map(|definition| definition.headword().to_string())
}

fn animals() -> StarDict {
    stardict(
        Some("m"),
        &[
            ("color", b"a hue"),
            ("l", b"a letter"),
            ("lead", b"to guide"),
            ("ox", b"a bovine"),
            ("oxen", b"a band"),
        ],
    )
}

#[test]
fn finds_the_headword_a_synonym_spells_another_way() {
    let dictionary = animals();
    let syn = synonyms(&[("colour", 0), ("mice", 3)]);
    let mut opened = open(&dictionary, Some(&syn));

    let definition = future::block_on(opened.lookup("colour")).unwrap().unwrap();

    assert_eq!(definition.headword(), "color");
    assert_eq!(definition.fields()[0].text(), "a hue");
    assert_eq!(headword(&mut opened, "Colour,").as_deref(), Some("color"));
}

#[test]
fn prefers_an_index_word_to_a_synonym() {
    let dictionary = animals();
    let syn = synonyms(&[("oxen", 3)]);
    let mut opened = open(&dictionary, Some(&syn));

    assert_eq!(headword(&mut opened, "oxen").as_deref(), Some("oxen"));
}

#[test]
fn prefers_a_synonym_to_an_english_form() {
    let dictionary = animals();
    let syn = synonyms(&[("led", 2)]);

    // "led" without its ending is "l"
    assert_eq!(
        headword(&mut open(&dictionary, None), "led").as_deref(),
        Some("l")
    );
    assert_eq!(
        headword(&mut open(&dictionary, Some(&syn)), "led").as_deref(),
        Some("lead"),
    );
}

#[test]
fn finds_synonyms_of_english_forms() {
    let dictionary = animals();
    let syn = synonyms(&[("colour", 0)]);
    let mut opened = open(&dictionary, Some(&syn));

    assert_eq!(headword(&mut opened, "colours").as_deref(), Some("color"));
}

fn numbered() -> (StarDict, Vec<u8>) {
    let words: Vec<String> = (0..2_000).map(|index| format!("word{index:05}")).collect();
    let entries: Vec<(&str, &[u8])> = words
        .iter()
        .map(|word| (word.as_str(), word.as_bytes()))
        .collect();

    let aliases: Vec<String> = (0..2_000).map(|index| format!("alias{index:05}")).collect();
    let syn: Vec<(&str, u32)> = aliases
        .iter()
        .zip(0..)
        .map(|(alias, ordinal)| (alias.as_str(), ordinal))
        .collect();

    (stardict(Some("m"), &entries), synonyms(&syn))
}

#[test]
fn finds_synonyms_through_samples() {
    let (dictionary, syn) = numbered();
    let mut opened = open(&dictionary, Some(&syn));

    future::block_on(opened.sample_index()).unwrap();

    for index in [0, 255, 256, 257, 1_999] {
        assert_eq!(
            headword(&mut opened, &format!("alias{index:05}")),
            Some(format!("word{index:05}")),
        );
    }
}

#[test]
fn round_trips_samples_with_synonyms() {
    let (dictionary, syn) = numbered();

    let saved = future::block_on(open(&dictionary, Some(&syn)).sample_index())
        .unwrap()
        .encode();

    let mut reopened = open(&dictionary, Some(&syn));

    assert!(reopened.use_samples(IndexSamples::decode(&saved).unwrap()));
    assert_eq!(
        headword(&mut reopened, "alias01234").as_deref(),
        Some("word01234"),
    );
}

#[test]
fn refuses_samples_taken_with_other_synonyms() {
    let (dictionary, syn) = numbered();

    let with = future::block_on(open(&dictionary, Some(&syn)).sample_index())
        .unwrap()
        .encode();
    let without = future::block_on(open(&dictionary, None).sample_index())
        .unwrap()
        .encode();

    let fewer = synonyms(&[("alias", 0)]);

    assert!(!open(&dictionary, None).use_samples(IndexSamples::decode(&with).unwrap()));
    assert!(!open(&dictionary, Some(&syn)).use_samples(IndexSamples::decode(&without).unwrap()));
    assert!(!open(&dictionary, Some(&fewer)).use_samples(IndexSamples::decode(&with).unwrap()));
}

#[test]
fn reports_a_synonym_past_the_index() {
    let dictionary = animals();
    let syn = synonyms(&[("cattle", 5)]);
    let mut opened = open(&dictionary, Some(&syn));

    assert!(matches!(
        future::block_on(opened.lookup("cattle")),
        Err(Error::<SliceSourceError>::MalformedIndex),
    ));
}

#[test]
fn finds_synonyms_without_samples() {
    let (dictionary, syn) = numbered();
    let mut opened = open(&dictionary, Some(&syn));

    assert_eq!(
        headword(&mut opened, "alias01999").as_deref(),
        Some("word01999"),
    );
}
