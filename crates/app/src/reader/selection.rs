//! The word held on a page, found by measuring the text under the finger, as
//! pages don't keep where each word sits.

use alloc::{string::String, vec, vec::Vec};
use core::ops::Range;

use inkpaper_reader::{Page, PageItem, ReadingPosition, Rect, TextFragment, TextMeasurer};

/// How far beside a line's text a press still picks its word.
const SLOP: u32 = 6;

/// How far a word's highlight reaches past it on a side with no punctuation
/// against it.
const HIGHLIGHT_PADDING: u32 = 2;

/// A word held on a page, to look up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WordSelection {
    /// the page it's on
    position: ReadingPosition,
    word: String,
    /// each text item the word is drawn in, by its index on the page, with
    /// the bounds of its highlight there: two for a word broken across lines
    pieces: Vec<(usize, Rect)>,
}

impl WordSelection {
    pub(crate) fn word(&self) -> &str {
        &self.word
    }

    pub(crate) const fn position(&self) -> ReadingPosition {
        self.position
    }

    pub(crate) fn pieces(&self) -> &[(usize, Rect)] {
        &self.pieces
    }
}

/// The word on `page` under `x` and `y`, which are page coordinates, measured
/// like the page was laid out.
pub(super) fn word_at<M: TextMeasurer>(
    page: &Page<'_>,
    x: u32,
    y: u32,
    measurer: &mut M,
) -> Option<WordSelection> {
    let items = page.items();

    // the text under the press, or else beside it
    let text_near = |slop| {
        items.iter().position(|item| match item {
            PageItem::Text(fragment) => near(fragment.bounds(), x, y, slop),
            PageItem::Image(_) => false,
        })
    };

    let index = text_near(0).or_else(|| text_near(SLOP))?;

    let fragment = text_item(items, index)?;
    let words = measure_words(fragment, measurer)?;

    // the word under the press, or the closest one beside it
    let picked = words
        .iter()
        .enumerate()
        .min_by_key(|(_, word)| distance(word, x))
        .map(|(picked, _)| picked)?;

    let text = fragment.text();
    let word = &words[picked];

    let mut selection = WordSelection {
        position: page.position(),
        word: String::from(&text[word.range.clone()]),
        pieces: vec![(index, word.bounds(fragment.bounds()))],
    };

    // a word broken at the end of its line goes on at the start of the next,
    // unless that's on the next page. The hyphen drawn at the break is
    // punctuation, so it isn't part of either piece
    if picked + 1 == words.len()
        && fragment.hyphenated()
        && let Some((next_index, next)) = neighbor(items, index, 1)
        && let Some(rest) = measure_words(next, measurer).and_then(|words| words.into_iter().next())
    {
        selection.word.push_str(&next.text()[rest.range.clone()]);
        selection
            .pieces
            .push((next_index, rest.bounds(next.bounds())));
    }

    if picked == 0
        && let Some((previous_index, previous)) = neighbor(items, index, -1)
        && previous.hyphenated()
        && let Some(start) =
            measure_words(previous, measurer).and_then(|words| words.into_iter().last())
    {
        selection
            .word
            .insert_str(0, &previous.text()[start.range.clone()]);
        selection
            .pieces
            .insert(0, (previous_index, start.bounds(previous.bounds())));
    }

    Some(selection)
}

/// A word of a text item, and where its highlight goes across the item.
struct Word {
    /// its bytes in the item's text
    range: Range<usize>,
    left: u32,
    right: u32,
}

impl Word {
    /// Its bounds on the page, the height of the item's line.
    fn bounds(&self, line: Rect) -> Rect {
        Rect::new(self.left, line.y(), self.right - self.left, line.height())
    }
}

/// The words of `fragment`, split at whitespace and without the punctuation
/// around them, each placed by measuring the text before it like the page
/// was laid out, its spaces stretched by the line's justification.
fn measure_words<M: TextMeasurer>(
    fragment: &TextFragment<'_>,
    measurer: &mut M,
) -> Option<Vec<Word>> {
    let text = fragment.text();
    let left = fragment.bounds().x();

    let mut advance = |end: usize| -> Option<u32> {
        let prefix = &text[..end];
        let spaces = prefix.matches(' ').count() as u32;
        let width = measurer.measure_text(prefix, fragment.style()).ok()?;

        Some(left + width + spaces * fragment.word_spacing())
    };

    let mut words = Vec::new();
    let mut start = None;

    let ends = text
        .char_indices()
        .map(|(offset, character)| (offset, character.is_whitespace()))
        .chain(core::iter::once((text.len(), true)));

    for (offset, whitespace) in ends {
        match (start, whitespace) {
            (None, false) => start = Some(offset),
            (Some(word_start), true) => {
                // a dash or a quote on its own isn't a word
                if let Some(range) = letters(text, word_start..offset) {
                    // padded only where it won't cover punctuation
                    let padding = |touching: bool| if touching { 0 } else { HIGHLIGHT_PADDING };

                    words.push(Word {
                        left: advance(range.start)?
                            .saturating_sub(padding(range.start > word_start)),
                        right: advance(range.end)? + padding(range.end < offset),
                        range,
                    });
                }

                start = None;
            }
            _ => {}
        }
    }

    Some(words)
}

/// `range` of `text` from its first letter or digit to its last, or `None`
/// without any.
fn letters(text: &str, range: Range<usize>) -> Option<Range<usize>> {
    let token = &text[range.clone()];

    let first = token.find(char::is_alphanumeric)?;
    let (last, character) = token
        .char_indices()
        .rfind(|(_, character)| character.is_alphanumeric())?;

    Some(range.start + first..range.start + last + character.len_utf8())
}

/// Whether a press at `x` and `y` lands on a line's text at `bounds`, or
/// within `slop` beside it.
fn near(bounds: Rect, x: u32, y: u32, slop: u32) -> bool {
    x + slop >= bounds.x()
        && x < bounds.x() + bounds.width() + slop
        && y >= bounds.y()
        && y < bounds.y() + bounds.height()
}

/// How far `x` is from `word`, nothing when on it.
fn distance(word: &Word, x: u32) -> u32 {
    word.left.saturating_sub(x) + x.saturating_sub(word.right)
}

fn text_item<'a, 'page>(
    items: &'a [PageItem<'page>],
    index: usize,
) -> Option<&'a TextFragment<'page>> {
    match items.get(index)? {
        PageItem::Text(fragment) => Some(fragment),
        PageItem::Image(_) => None,
    }
}

/// The text item right after `index` (`step` 1) or right before it (-1), and
/// its index.
fn neighbor<'a, 'page>(
    items: &'a [PageItem<'page>],
    index: usize,
    step: isize,
) -> Option<(usize, &'a TextFragment<'page>)> {
    let neighbor = index.checked_add_signed(step)?;

    text_item(items, neighbor).map(|fragment| (neighbor, fragment))
}
