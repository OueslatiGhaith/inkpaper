use alloc::{string::String, vec::Vec};
use core::cmp::Ordering;

use crate::index::compare_folded;

/// The spellings to look `word` up by, in order: the word as a page gives it
/// without the punctuation around it, in lowercase when it has capitals
/// beyond ASCII, then the English forms it could be inflected from. The
/// endings are crosspoint's.
pub(crate) fn word_forms(word: &str) -> Vec<String> {
    let mut forms = Vec::new();

    let word = clean(word);

    if word.is_empty() {
        return forms;
    }

    // ASCII letters match in any case already
    let lowercase = word.to_lowercase();

    push(&mut forms, word.clone());

    if compare_folded(lowercase.as_bytes(), word.as_bytes()) != Ordering::Equal {
        push(&mut forms, lowercase);
    }

    for (suffix, replacements) in ENDINGS {
        let Some(stem) = strip_suffix(&word, suffix) else {
            continue;
        };

        for replacement in *replacements {
            push(&mut forms, [stem, replacement].concat());
        }

        // a doubled consonant: stopped, running
        if matches!(*suffix, "ed" | "ing")
            && let [.., before, last] = stem.as_bytes()
            && before.eq_ignore_ascii_case(last)
            && last.is_ascii_alphabetic()
        {
            push(&mut forms, String::from(&stem[..stem.len() - 1]));
        }
    }

    forms
}

/// The endings to take off, each with what to put back.
const ENDINGS: &[(&str, &[&str])] = &[
    // the cat's
    ("'s", &[""]),
    // stories
    ("ies", &["y"]),
    // boxes
    ("es", &[""]),
    // dogs
    ("s", &[""]),
    // walked, loved
    ("ed", &["", "e"]),
    // walking, making
    ("ing", &["", "e"]),
];

/// `word` without the spaces and punctuation around it, like a sentence's
/// quotes and full stop, or soft hyphens inside it. A typographic apostrophe
/// becomes the ASCII one dictionaries spell with.
fn clean(word: &str) -> String {
    word.trim_matches(|character: char| !character.is_alphanumeric())
        .chars()
        .filter(|&character| character != '\u{AD}')
        .map(|character| match character {
            '\u{2019}' => '\'',
            character => character,
        })
        .collect()
}

/// `word` without `suffix` in any ASCII case, leaving something behind.
fn strip_suffix<'a>(word: &'a str, suffix: &str) -> Option<&'a str> {
    let split = word
        .len()
        .checked_sub(suffix.len())
        .filter(|&split| split > 0)?;

    (word.is_char_boundary(split) && word[split..].eq_ignore_ascii_case(suffix))
        .then(|| &word[..split])
}

fn push(forms: &mut Vec<String>, form: String) {
    if !forms.contains(&form) {
        forms.push(form);
    }
}
