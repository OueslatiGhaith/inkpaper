//! The words held on a page, found by measuring the text under the finger,
//! as pages don't keep where each word sits.

use alloc::{string::String, vec::Vec};
use core::ops::Range;

use inkpaper_reader::{Page, PageItem, ReadingPosition, Rect, TextFragment, TextMeasurer};

/// How far beside a line's text a press still picks its word.
const SLOP: u32 = 6;

/// How far a word's highlight reaches past it on a side with no punctuation
/// against it.
const HIGHLIGHT_PADDING: u32 = 2;

/// How long a selection can grow, the most a dictionary's word can be.
const MAX_SELECTION_BYTES: usize = 255;

/// Words held on a page, to look up: the one held, and the ones a drag from
/// it went over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WordSelection {
    /// the page it's on
    position: ReadingPosition,
    word: String,
    /// each text item the words are drawn in, by its index on the page,
    /// with the bounds of its highlight there: one a line
    pieces: Vec<(usize, Rect)>,
    /// the word held
    anchor: WordSpot,
    /// the word the drag reached, or the one held
    end: WordSpot,
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

/// A word on a page: its text item's index, and its index among the item's
/// words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct WordSpot {
    item: usize,
    word: usize,
}

/// The word on `page` under `x` and `y`, which are page coordinates, measured
/// like the page was laid out.
pub(super) fn word_at<M: TextMeasurer>(
    page: &Page<'_>,
    x: u32,
    y: u32,
    measurer: &mut M,
) -> Option<WordSelection> {
    let spot = spot_at(page.items(), x, y, measurer)?;

    select(page, spot, spot, measurer)
}

/// `selection` grown or shrunk to the word under `x` and `y` on `page`, or
/// `None` when that's no other word, or the words would be too long to look
/// up.
pub(super) fn extend_to<M: TextMeasurer>(
    selection: &WordSelection,
    page: &Page<'_>,
    x: u32,
    y: u32,
    measurer: &mut M,
) -> Option<WordSelection> {
    let end = spot_at(page.items(), x, y, measurer)?;

    if end == selection.end {
        return None;
    }

    select(page, selection.anchor, end, measurer)
        .filter(|extended| extended.word.len() <= MAX_SELECTION_BYTES)
}

/// The word under `x` and `y`, or the closest one beside it on its line.
fn spot_at<M: TextMeasurer>(
    items: &[PageItem<'_>],
    x: u32,
    y: u32,
    measurer: &mut M,
) -> Option<WordSpot> {
    // the text under the press, or else beside it
    let text_near = |slop| {
        items.iter().position(|item| match item {
            PageItem::Text(fragment) => near(fragment.bounds(), x, y, slop),
            PageItem::Image(_) => false,
        })
    };

    let item = text_near(0).or_else(|| text_near(SLOP))?;
    let words = measure_words(text_item(items, item)?, measurer)?;

    let word = words
        .iter()
        .enumerate()
        .min_by_key(|(_, word)| distance(word, x))
        .map(|(word, _)| word)?;

    Some(WordSpot { item, word })
}

/// The words from `anchor` to `end`, in either order, with the rest of a
/// word either one is part of where it's broken across lines.
fn select<M: TextMeasurer>(
    page: &Page<'_>,
    anchor: WordSpot,
    end: WordSpot,
    measurer: &mut M,
) -> Option<WordSelection> {
    let items = page.items();

    let mut first = anchor.min(end);
    let mut last = anchor.max(end);

    // a word broken at the end of its line goes on at the start of the
    // next, unless that's on the next page
    if first.word == 0
        && let Some((previous, fragment)) = neighbor(items, first.item, -1)
        && fragment.hyphenated()
        && let Some(words) = measure_words(fragment, measurer)
        && !words.is_empty()
    {
        first = WordSpot {
            item: previous,
            word: words.len() - 1,
        };
    }

    let fragment = text_item(items, last.item)?;

    if fragment.hyphenated()
        && measure_words(fragment, measurer)?.len() == last.word + 1
        && let Some((next, fragment)) = neighbor(items, last.item, 1)
        && measure_words(fragment, measurer).is_some_and(|words| !words.is_empty())
    {
        last = WordSpot {
            item: next,
            word: 0,
        };
    }

    let mut word = String::new();
    let mut pieces = Vec::new();
    // whether the item before ended in a hyphen drawn at its break, which
    // is punctuation, so its word goes on without it
    let mut broken = false;

    for item in first.item..=last.item {
        let Some(fragment) = text_item(items, item) else {
            continue;
        };

        let words = measure_words(fragment, measurer)?;
        let from = if item == first.item { first.word } else { 0 };
        let to = if item == last.item {
            last.word
        } else {
            words.len().saturating_sub(1)
        };

        let Some(selected) = words.get(from..=to).filter(|words| !words.is_empty()) else {
            continue;
        };

        let (start, finish) = (&selected[0], &selected[selected.len() - 1]);

        if !word.is_empty() && !broken {
            word.push(' ');
        }

        // the line as written between them, punctuation and all
        word.push_str(&fragment.text()[start.range.start..finish.range.end]);
        let line = fragment.bounds();

        pieces.push((
            item,
            Rect::new(
                start.left,
                line.y(),
                finish.right - start.left,
                line.height(),
            ),
        ));

        broken = fragment.hyphenated() && to + 1 == words.len();
    }

    (!pieces.is_empty()).then(|| WordSelection {
        position: page.position(),
        word,
        pieces,
        anchor,
        end,
    })
}

/// A word of a text item, and where its highlight goes across the item.
struct Word {
    /// its bytes in the item's text
    range: Range<usize>,
    left: u32,
    right: u32,
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
