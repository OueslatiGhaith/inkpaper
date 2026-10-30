use icu_properties::{
    CodePointMapData, CodePointMapDataBorrowed,
    props::{BidiClass, BidiMirroringGlyph, BidiPairedBracketType},
};

use crate::{FontRegistry, Offset};

use super::{ShapeError, ShapedGlyph, TextDirection};

const DIRECTIONAL_RUN_CAPACITY: usize = 32;
/// lines with more bracket pairs, or deeper nesting, leave their brackets as plain neutrals
const BRACKET_PAIR_CAPACITY: usize = 32;

const BIDI_CLASSES: CodePointMapDataBorrowed<'static, BidiClass> =
    CodePointMapData::<BidiClass>::new();
const MIRRORING_GLYPHS: CodePointMapDataBorrowed<'static, BidiMirroringGlyph> =
    CodePointMapData::<BidiMirroringGlyph>::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
enum DirectionalClass {
    LeftToRight,
    RightToLeft,
    Number,
    Neutral,
}

/// a run of a line's text in one direction, from its byte offset up to the next run's.
/// Offsets from the text's length on belong to the glyphs shaped after the text, such as
/// an ellipsis
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TextRun {
    start: usize,
    class: DirectionalClass,
    context_direction: TextDirection,
    level: u8,
}

impl TextRun {
    const EMPTY: Self = Self::new(0, DirectionalClass::Neutral);

    const fn new(start: usize, class: DirectionalClass) -> Self {
        Self {
            start,
            class,
            context_direction: TextDirection::LeftToRight,
            level: 0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct BracketPair {
    open: usize,
    close: usize,
    /// `None` when there is no strong text between the brackets
    direction: Option<TextDirection>,
}

impl BracketPair {
    const EMPTY: Self = Self {
        open: 0,
        close: 0,
        direction: None,
    };

    const fn contains_bracket(self, offset: usize) -> bool {
        self.open == offset || self.close == offset
    }
}

/// the direction of a line's text: the direction of its first strong character
pub(crate) fn paragraph_direction(text: &str) -> TextDirection {
    for character in text.chars() {
        match directional_class(character) {
            DirectionalClass::LeftToRight => return TextDirection::LeftToRight,
            DirectionalClass::RightToLeft => return TextDirection::RightToLeft,
            DirectionalClass::Number | DirectionalClass::Neutral => {}
        }
    }

    TextDirection::LeftToRight
}

/// a glyph's cluster in the piece it was shaped from, given its offset in the line
pub(super) fn piece_cluster(text: &str, line_offset: usize) -> usize {
    if line_offset >= text.len() {
        line_offset - text.len()
    } else {
        line_offset
    }
}

/// splits a line into directional runs, from the line offsets of its glyphs in logical
/// order
pub(super) struct TextRunBuilder<'text> {
    text: &'text str,
    paragraph_direction: TextDirection,
    /// without right-to-left text every glyph resolves to level 0, so the text is one
    /// run however often words, numbers and punctuation alternate
    single_run: bool,
    brackets: [BracketPair; BRACKET_PAIR_CAPACITY],
    bracket_count: usize,
    runs: [TextRun; DIRECTIONAL_RUN_CAPACITY],
    written: usize,
}

impl<'text> TextRunBuilder<'text> {
    pub(super) fn new(text: &'text str, paragraph_direction: TextDirection) -> Self {
        let single_run = !text
            .chars()
            .any(|character| directional_class(character) == DirectionalClass::RightToLeft);

        let mut builder = Self {
            text,
            paragraph_direction,
            single_run,
            brackets: [BracketPair::EMPTY; BRACKET_PAIR_CAPACITY],
            bracket_count: 0,
            runs: [TextRun::EMPTY; DIRECTIONAL_RUN_CAPACITY],
            written: 0,
        };

        if single_run {
            builder.runs[0] = TextRun::new(0, DirectionalClass::LeftToRight);
            builder.written = 1;
        } else {
            builder.bracket_count = find_bracket_pairs(text, &mut builder.brackets).unwrap_or(0);
            resolve_bracket_pairs(
                text,
                paragraph_direction,
                &mut builder.brackets[..builder.bracket_count],
            );
        }

        builder
    }

    /// whether the runs are known without the glyphs' offsets
    pub(super) const fn is_single_run(&self) -> bool {
        self.single_run
    }

    /// adds the next glyph, at `offset` in the line
    pub(super) fn push(&mut self, offset: usize) -> Result<(), ShapeError> {
        if self.single_run {
            return Ok(());
        }

        let class = if offset < self.text.len() {
            directional_class_for_cluster(self.text, offset, &self.brackets[..self.bracket_count])
        } else {
            // the glyphs after the text, currently the renderer's ellipsis suffix, aren't
            // part of it. Treat them as neutrals and let the paragraph context place them
            DirectionalClass::Neutral
        };

        let written = self.written;
        let output = &mut self.runs;

        if written > 0 && output[written - 1].class == class {
            return Ok(());
        }

        // a neutral between two runs of the same strong direction resolves to that
        // direction and is merged with them later, so fold it in now instead of
        // spending two runs on every space between words
        let strong = matches!(
            class,
            DirectionalClass::LeftToRight | DirectionalClass::RightToLeft
        );

        if strong
            && written >= 2
            && output[written - 1].class == DirectionalClass::Neutral
            && output[written - 2].class == class
        {
            self.written -= 1;
            return Ok(());
        }

        let Some(destination) = output.get_mut(written) else {
            return Err(ShapeError::TooManyDirectionalRuns);
        };

        *destination = TextRun::new(offset, class);
        self.written += 1;

        Ok(())
    }

    /// resolves the runs' levels and merges the runs that share one
    pub(super) fn finish(mut self) -> LineRuns {
        let runs = &mut self.runs[..self.written];

        resolve_directional_run_levels(runs, self.paragraph_direction);
        let len = merge_same_level_runs(runs);

        LineRuns {
            runs: self.runs,
            len,
        }
    }
}

/// the directional runs of a line, in logical order
pub(super) struct LineRuns {
    runs: [TextRun; DIRECTIONAL_RUN_CAPACITY],
    len: usize,
}

impl LineRuns {
    /// the line offsets of run `index`'s glyphs
    pub(super) fn range(&self, index: usize) -> core::ops::Range<usize> {
        let end = match index + 1 < self.len {
            true => self.runs[index + 1].start,
            false => usize::MAX,
        };

        self.runs[index].start..end
    }

    pub(super) fn level(&self, index: usize) -> u8 {
        self.runs[index].level
    }

    /// the runs' indices in visual order, reversing each span of runs at or above every
    /// level down to the lowest odd one (UAX #9 L2)
    pub(super) fn visual_order(&self) -> ([u8; DIRECTIONAL_RUN_CAPACITY], usize) {
        let mut order = [0u8; DIRECTIONAL_RUN_CAPACITY];
        for (index, slot) in order[..self.len].iter_mut().enumerate() {
            *slot = index as u8;
        }

        let levels = &self.runs[..self.len];
        let highest_level = levels.iter().map(|run| run.level).max().unwrap_or(0);
        let lowest_odd_level = levels
            .iter()
            .map(|run| run.level)
            .filter(|level| level % 2 == 1)
            .min();

        if let Some(lowest_odd_level) = lowest_odd_level {
            let order = &mut order[..self.len];
            let level_at =
                |order: &[u8], position: usize| self.runs[order[position] as usize].level;

            for level in (lowest_odd_level..=highest_level).rev() {
                let mut cursor = 0usize;

                while cursor < order.len() {
                    if level_at(order, cursor) < level {
                        cursor += 1;
                        continue;
                    }

                    let start = cursor;
                    while cursor < order.len() && level_at(order, cursor) >= level {
                        cursor += 1;
                    }

                    order[start..cursor].reverse();
                }
            }
        }

        (order, self.len)
    }
}

fn directional_class_for_cluster(
    text: &str,
    cluster: usize,
    brackets: &[BracketPair],
) -> DirectionalClass {
    let Some(remaining) = text.get(cluster..) else {
        return DirectionalClass::Neutral;
    };
    let Some(character) = remaining.chars().next() else {
        return DirectionalClass::Neutral;
    };

    let class = directional_class(character);

    // a resolved bracket acts as a strong character from here on
    if class == DirectionalClass::Neutral
        && let Some(direction) = brackets
            .iter()
            .find(|pair| pair.contains_bracket(cluster))
            .and_then(|pair| pair.direction)
    {
        return match direction {
            TextDirection::LeftToRight => DirectionalClass::LeftToRight,
            TextDirection::RightToLeft => DirectionalClass::RightToLeft,
        };
    }

    class
}

/// pairs each closing bracket with the nearest open bracket of the same kind (UAX #9 BD16),
/// sorted by opening position.
///
/// `None` when the brackets don't fit in `pairs`
fn find_bracket_pairs(
    text: &str,
    pairs: &mut [BracketPair; BRACKET_PAIR_CAPACITY],
) -> Option<usize> {
    // the opening bracket's position and the closing bracket it expects
    let mut openers = [(0usize, '\0'); BRACKET_PAIR_CAPACITY];
    let mut depth = 0usize;
    let mut count = 0usize;

    for (offset, character) in text.char_indices() {
        let mirroring = MIRRORING_GLYPHS.get(character);

        match mirroring.paired_bracket_type {
            BidiPairedBracketType::Open => {
                if BIDI_CLASSES.get(character) != BidiClass::OtherNeutral {
                    continue;
                }

                let Some(closing) = mirroring.mirroring_glyph else {
                    continue;
                };

                *openers.get_mut(depth)? = (offset, canonical_bracket(closing));
                depth += 1;
            }

            BidiPairedBracketType::Close => {
                let closing = canonical_bracket(character);

                // an unmatched closing bracket is ignored, and closing an outer pair also
                // drops the unclosed brackets inside it
                let Some(index) = openers[..depth]
                    .iter()
                    .rposition(|(_, expected)| *expected == closing)
                else {
                    continue;
                };

                if count == BRACKET_PAIR_CAPACITY {
                    return None;
                }

                let open = openers[index].0;
                let position = pairs[..count].partition_point(|pair| pair.open < open);

                pairs.copy_within(position..count, position + 1);
                pairs[position] = BracketPair {
                    open,
                    close: offset,
                    direction: None,
                };

                count += 1;
                depth = index;
            }

            _ => {}
        }
    }

    Some(count)
}

/// U+2329 and U+232A are canonically equivalent to U+3008 and U+3009, so they pair
/// with each other
const fn canonical_bracket(character: char) -> char {
    match character {
        '\u{2329}' => '\u{3008}',
        '\u{232A}' => '\u{3009}',
        _ => character,
    }
}

/// gives each bracket pair the direction of the text it encloses (UAX #9 N0), so the
/// brackets stay with that text
fn resolve_bracket_pairs(
    text: &str,
    paragraph_direction: TextDirection,
    pairs: &mut [BracketPair],
) {
    for index in 0..pairs.len() {
        let pair = pairs[index];

        let Some(inside) = text.get(pair.open..pair.close) else {
            continue;
        };

        let mut previous_strong =
            strong_direction_before(text, pair.open).unwrap_or(paragraph_direction);
        let mut found_paragraph_direction = false;
        let mut found_opposite_direction = false;

        for character in inside.chars() {
            let direction = match BIDI_CLASSES.get(character) {
                BidiClass::LeftToRight => {
                    previous_strong = TextDirection::LeftToRight;
                    previous_strong
                }
                BidiClass::RightToLeft | BidiClass::ArabicLetter => {
                    previous_strong = TextDirection::RightToLeft;
                    previous_strong
                }
                BidiClass::ArabicNumber => TextDirection::RightToLeft,
                // a European number after left-to-right text is left-to-right, otherwise
                // it counts as right-to-left (W7)
                BidiClass::EuropeanNumber => previous_strong,
                _ => continue,
            };

            if direction == paragraph_direction {
                found_paragraph_direction = true;
                break;
            }

            found_opposite_direction = true;
        }

        pairs[index].direction = if found_paragraph_direction {
            Some(paragraph_direction)
        } else if found_opposite_direction {
            // only opposite text inside: the brackets follow it when the text before
            // them runs the same way
            Some(
                bracket_context_before(text, pair.open, &pairs[..index])
                    .unwrap_or(paragraph_direction),
            )
        } else {
            None
        };
    }
}

/// the direction of the last strong character before `offset`
fn strong_direction_before(text: &str, offset: usize) -> Option<TextDirection> {
    let before = text.get(..offset)?;

    before
        .chars()
        .rev()
        .find_map(|character| match BIDI_CLASSES.get(character) {
            BidiClass::LeftToRight => Some(TextDirection::LeftToRight),
            BidiClass::RightToLeft | BidiClass::ArabicLetter => Some(TextDirection::RightToLeft),
            _ => None,
        })
}

/// the direction of the text before a bracket pair, for N0. Arabic numbers count as
/// right-to-left, and the brackets resolved so far as strong
fn bracket_context_before(
    text: &str,
    offset: usize,
    resolved: &[BracketPair],
) -> Option<TextDirection> {
    let before = text.get(..offset)?;

    for (position, character) in before.char_indices().rev() {
        match BIDI_CLASSES.get(character) {
            BidiClass::LeftToRight => return Some(TextDirection::LeftToRight),
            BidiClass::RightToLeft | BidiClass::ArabicLetter | BidiClass::ArabicNumber => {
                return Some(TextDirection::RightToLeft);
            }
            BidiClass::OtherNeutral => {
                if let Some(direction) = resolved
                    .iter()
                    .find(|pair| pair.contains_bracket(position))
                    .and_then(|pair| pair.direction)
                {
                    return Some(direction);
                }
            }
            // a European number takes the direction of the strong text before it (W7),
            // so keep looking
            _ => {}
        }
    }

    None
}

fn directional_class(character: char) -> DirectionalClass {
    match BIDI_CLASSES.get(character) {
        BidiClass::LeftToRight => DirectionalClass::LeftToRight,
        BidiClass::RightToLeft | BidiClass::ArabicLetter => DirectionalClass::RightToLeft,
        BidiClass::EuropeanNumber | BidiClass::ArabicNumber => DirectionalClass::Number,
        _ => DirectionalClass::Neutral,
    }
}

fn resolve_directional_run_levels(runs: &mut [TextRun], paragraph_direction: TextDirection) {
    for index in 0..runs.len() {
        let context_direction = match runs[index].class {
            DirectionalClass::LeftToRight => TextDirection::LeftToRight,
            DirectionalClass::RightToLeft => TextDirection::RightToLeft,
            DirectionalClass::Number => previous_strong_direction(runs, index)
                .or_else(|| next_strong_direction(runs, index))
                .unwrap_or(paragraph_direction),
            DirectionalClass::Neutral => paragraph_direction,
        };

        runs[index].context_direction = context_direction;
    }

    for index in 0..runs.len() {
        if runs[index].class != DirectionalClass::Neutral {
            continue;
        }

        let left = previous_context_direction(runs, index, paragraph_direction);
        let right = next_context_direction(runs, index, paragraph_direction);

        runs[index].context_direction = if left == right {
            left
        } else {
            paragraph_direction
        };
    }

    for run in runs {
        run.level = match run.class {
            DirectionalClass::Number => number_level(paragraph_direction, run.context_direction),
            _ => direction_level(paragraph_direction, run.context_direction),
        };
    }
}

/// mirrors the characters of whole clusters of a right-to-left run, in logical order
/// with their line offsets as clusters. Glyphs after the text are left alone
pub(super) fn mirror_clusters<const FONTS: usize>(
    registry: &FontRegistry<'_, FONTS>,
    size_px: u16,
    text: &str,
    glyphs: &mut [ShapedGlyph],
) {
    let mut previous_cluster = None;

    for glyph in glyphs.iter_mut() {
        let shaped = *glyph;
        let cluster = shaped.cluster();

        // multiple glyphs can share a shaping cluster, such as an Arabic base
        // + transparent mark. Mirroring is a source-character operation,
        // so process the cluster once.
        if previous_cluster == Some(cluster) {
            continue;
        }

        previous_cluster = Some(cluster);

        let Some(remaining) = text.get(cluster..) else {
            continue;
        };
        let Some(character) = remaining.chars().next() else {
            continue;
        };
        let Some(mirrored) = mirrored_character(character) else {
            continue;
        };

        // prefer the face that produced the original glyph, then use the normal
        // registered fallback order.
        // do not use replacement fallback here: if the actual mirrored character
        // doesn't exist, keeping the original glyph is better than rendering '?'.
        let Some((resolved, base_advance)) = registry.resolve_character_with_advance_exact(
            shaped.font_instance(),
            mirrored,
            size_px,
        ) else {
            continue;
        };

        *glyph = ShapedGlyph::new_with_instance(
            resolved.font_instance(),
            resolved.glyph(),
            shaped.cluster(),
            base_advance,
            Offset::ZERO,
            base_advance,
        );
    }
}

fn mirrored_character(character: char) -> Option<char> {
    MIRRORING_GLYPHS.get(character).mirroring_glyph
}

fn previous_strong_direction(runs: &[TextRun], index: usize) -> Option<TextDirection> {
    for run in runs[..index].iter().rev() {
        match run.class {
            DirectionalClass::LeftToRight => return Some(TextDirection::LeftToRight),
            DirectionalClass::RightToLeft => return Some(TextDirection::RightToLeft),
            DirectionalClass::Number | DirectionalClass::Neutral => {}
        }
    }

    None
}

fn next_strong_direction(runs: &[TextRun], index: usize) -> Option<TextDirection> {
    for run in runs[index.saturating_add(1)..].iter() {
        match run.class {
            DirectionalClass::LeftToRight => return Some(TextDirection::LeftToRight),
            DirectionalClass::RightToLeft => return Some(TextDirection::RightToLeft),
            DirectionalClass::Number | DirectionalClass::Neutral => {}
        }
    }

    None
}

fn previous_context_direction(
    runs: &[TextRun],
    index: usize,
    fallback: TextDirection,
) -> TextDirection {
    for run in runs[..index].iter().rev() {
        if run.class != DirectionalClass::Neutral {
            return run.context_direction;
        }
    }

    fallback
}

fn next_context_direction(
    runs: &[TextRun],
    index: usize,
    fallback: TextDirection,
) -> TextDirection {
    for run in runs[index.saturating_add(1)..].iter() {
        if run.class != DirectionalClass::Neutral {
            return run.context_direction;
        }
    }

    fallback
}

fn direction_level(paragraph_direction: TextDirection, run_direction: TextDirection) -> u8 {
    match (paragraph_direction, run_direction) {
        (TextDirection::LeftToRight, TextDirection::LeftToRight) => 0,
        (TextDirection::LeftToRight, TextDirection::RightToLeft) => 1,
        (TextDirection::RightToLeft, TextDirection::RightToLeft) => 1,
        (TextDirection::RightToLeft, TextDirection::LeftToRight) => 2,
    }
}

fn number_level(paragraph_direction: TextDirection, context_direction: TextDirection) -> u8 {
    if paragraph_direction == TextDirection::RightToLeft
        || context_direction == TextDirection::RightToLeft
    {
        2
    } else {
        0
    }
}

fn merge_same_level_runs(runs: &mut [TextRun]) -> usize {
    if runs.is_empty() {
        return 0;
    }

    let mut written = 1usize;

    for read in 1..runs.len() {
        let run = runs[read];

        // the earlier run reaches up to the next start it keeps
        if runs[written - 1].level == run.level {
            continue;
        }

        runs[written] = run;
        written = written.saturating_add(1);
    }

    written
}

pub(super) fn reverse_glyph_clusters(glyphs: &mut [ShapedGlyph]) {
    glyphs.reverse();

    let mut start = 0usize;

    while start < glyphs.len() {
        let cluster = glyphs[start].cluster();
        let mut end = start.saturating_add(1);

        while end < glyphs.len() && glyphs[end].cluster() == cluster {
            end = end.saturating_add(1);
        }

        // reversing the complete span changed both cluster order and the order of glyphs
        // inside each cluster. Reverse each cluster again so only the cluster sequence changes.
        glyphs[start..end].reverse();

        start = end;
    }
}
