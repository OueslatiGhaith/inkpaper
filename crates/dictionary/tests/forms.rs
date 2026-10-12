mod support;

use futures_lite::future;
use inkpaper_dictionary::{Dictionary, DictionaryInfo};
use inkpaper_epub::SliceSource;

use support::{StarDict, stardict};

fn headword(dictionary: &StarDict, word: &str) -> Option<String> {
    let info = DictionaryInfo::parse(dictionary.ifo.as_bytes()).unwrap();

    let mut dictionary = future::block_on(Dictionary::open(
        info,
        SliceSource::new(&dictionary.idx),
        SliceSource::new(&dictionary.dict),
    ))
    .unwrap();

    future::block_on(dictionary.lookup(word))
        .unwrap()
        .map(|definition| definition.headword().to_string())
}

fn english() -> StarDict {
    stardict(
        Some("m"),
        &[
            ("box", b"a container"),
            ("cat", b"a small feline"),
            ("dog", b"a canine"),
            ("dogs", b"more than one canine"),
            ("don't", b"do not"),
            ("love", b"to care deeply"),
            ("make", b"to create"),
            ("run", b"to move fast"),
            ("stop", b"to halt"),
            ("story", b"a tale"),
            ("walk", b"to move on foot"),
            ("well-being", b"health"),
        ],
    )
}

#[test]
fn drops_the_punctuation_around_a_word() {
    let dictionary = english();

    assert_eq!(headword(&dictionary, "“cat,”").as_deref(), Some("cat"));
    assert_eq!(headword(&dictionary, " (dog). ").as_deref(), Some("dog"));
    assert_eq!(headword(&dictionary, "—walk…").as_deref(), Some("walk"));
}

#[test]
fn keeps_the_punctuation_inside_a_word() {
    let dictionary = english();

    assert_eq!(
        headword(&dictionary, "well-being.").as_deref(),
        Some("well-being"),
    );
    assert_eq!(headword(&dictionary, "don't").as_deref(), Some("don't"));
}

#[test]
fn reads_a_typographic_apostrophe_as_an_ascii_one() {
    assert_eq!(headword(&english(), "don’t").as_deref(), Some("don't"));
}

#[test]
fn ignores_soft_hyphens() {
    assert_eq!(
        headword(&english(), "sto\u{AD}ry").as_deref(),
        Some("story")
    );
}

#[test]
fn finds_the_word_an_english_form_comes_from() {
    let dictionary = english();

    for (word, expected) in [
        ("cat's", "cat"),
        ("cat’s", "cat"),
        ("boxes", "box"),
        ("stories", "story"),
        ("Stories", "story"),
        ("walked", "walk"),
        ("loved", "love"),
        ("stopped", "stop"),
        ("walking", "walk"),
        ("making", "make"),
        ("running", "run"),
        ("CATS", "cat"),
    ] {
        assert_eq!(
            headword(&dictionary, word).as_deref(),
            Some(expected),
            "{word}"
        );
    }
}

#[test]
fn prefers_the_word_as_written_to_its_stem() {
    assert_eq!(headword(&english(), "dogs").as_deref(), Some("dogs"));
}

#[test]
fn matches_capitals_beyond_ascii() {
    let dictionary = stardict(Some("m"), &[("éclair", b"a pastry"), ("über", b"above")]);

    assert_eq!(headword(&dictionary, "Éclair").as_deref(), Some("éclair"));
    assert_eq!(headword(&dictionary, "ÜBER").as_deref(), Some("über"));
}

#[test]
fn misses_words_without_letters_or_a_known_form() {
    let dictionary = english();

    assert_eq!(headword(&dictionary, "“…”"), None);
    assert_eq!(headword(&dictionary, ""), None);
    assert_eq!(headword(&dictionary, "s"), None);
    assert_eq!(headword(&dictionary, "catalogues"), None);
}
