use icu_properties::{
    CodePointMapData, CodePointMapDataBorrowed,
    props::{BidiClass, BidiMirroringGlyph, BidiPairedBracketType},
};

use crate::{FontRegistry, Offset, PreparedFont, pair_cache::PairPositioningCache, px};

use super::{
    ShapeError, ShapedGlyph, ShapedRun, SimpleShaper, TextDirection,
    positioning::apply_visual_positioning,
};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DirectionalRun {
    pub(super) start: usize,
    pub(super) end: usize,
    class: DirectionalClass,
    context_direction: TextDirection,
    pub(super) level: u8,
}

impl DirectionalRun {
    const EMPTY: Self = Self {
        start: 0,
        end: 0,
        class: DirectionalClass::Neutral,
        context_direction: TextDirection::LeftToRight,
        level: 0,
    };

    const fn new(start: usize, end: usize, class: DirectionalClass) -> Self {
        Self {
            start,
            end,
            class,
            context_direction: TextDirection::LeftToRight,
            level: 0,
        }
    }

    const fn len(self) -> usize {
        self.end.saturating_sub(self.start)
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

impl SimpleShaper {
    pub fn visual_order<'out, const FONTS: usize>(
        &self,
        registry: &FontRegistry<'_, FONTS>,
        size_px: u16,
        text: &str,
        text_glyph_count: usize,
        glyphs: &'out mut [ShapedGlyph],
    ) -> Result<ShapedRun<'out>, ShapeError> {
        self.visual_order_impl(
            registry,
            size_px,
            text,
            text_glyph_count,
            glyphs,
            None::<&mut PairPositioningCache<1>>,
            None,
        )
    }

    pub(super) fn visual_order_with_prepared_font<'out, 'font, const FONTS: usize>(
        &self,
        registry: &'font FontRegistry<'_, FONTS>,
        size_px: u16,
        text: &str,
        text_glyph_count: usize,
        glyphs: &'out mut [ShapedGlyph],
        prepared_font: Option<&PreparedFont<'font>>,
    ) -> Result<ShapedRun<'out>, ShapeError> {
        self.visual_order_impl(
            registry,
            size_px,
            text,
            text_glyph_count,
            glyphs,
            None::<&mut PairPositioningCache<1>>,
            prepared_font,
        )
    }

    #[cfg(feature = "eink")]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn visual_order_with_pair_positioning_cache<
        'out,
        'font,
        const FONTS: usize,
        const SLOTS: usize,
    >(
        &self,
        registry: &'font FontRegistry<'_, FONTS>,
        size_px: u16,
        text: &str,
        text_glyph_count: usize,
        glyphs: &'out mut [ShapedGlyph],
        prepared_font: Option<&PreparedFont<'font>>,
        pair_positioning_cache: &mut PairPositioningCache<SLOTS>,
    ) -> Result<ShapedRun<'out>, ShapeError> {
        self.visual_order_impl(
            registry,
            size_px,
            text,
            text_glyph_count,
            glyphs,
            Some(pair_positioning_cache),
            prepared_font,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn visual_order_impl<'out, 'font, const FONTS: usize, const SLOTS: usize>(
        &self,
        registry: &'font FontRegistry<'_, FONTS>,
        size_px: u16,
        text: &str,
        text_glyph_count: usize,
        glyphs: &'out mut [ShapedGlyph],
        pair_positioning_cache: Option<&mut PairPositioningCache<SLOTS>>,
        prepared_font: Option<&PreparedFont<'font>>,
    ) -> Result<ShapedRun<'out>, ShapeError> {
        let direction = paragraph_direction(text);

        if glyphs.is_empty() {
            return Ok(ShapedRun::new(glyphs, direction, px(0)));
        }

        let text_glyph_count = text_glyph_count.min(glyphs.len());
        let mut runs = [DirectionalRun::EMPTY; DIRECTIONAL_RUN_CAPACITY];

        let run_count =
            build_directional_runs(text, direction, text_glyph_count, glyphs, &mut runs)?;

        resolve_directional_run_levels(&mut runs[..run_count], direction);

        mirror_odd_level_glyphs(
            registry,
            size_px,
            text,
            text_glyph_count,
            glyphs,
            &runs[..run_count],
        );

        let run_count = merge_same_level_runs(&mut runs[..run_count]);

        reorder_directional_runs(glyphs, &mut runs[..run_count]);

        let advance = apply_visual_positioning(
            registry,
            size_px,
            glyphs,
            &runs[..run_count],
            prepared_font,
            pair_positioning_cache,
        );

        Ok(ShapedRun::new(glyphs, direction, advance))
    }
}

fn paragraph_direction(text: &str) -> TextDirection {
    for character in text.chars() {
        match directional_class(character) {
            DirectionalClass::LeftToRight => return TextDirection::LeftToRight,
            DirectionalClass::RightToLeft => return TextDirection::RightToLeft,
            DirectionalClass::Number | DirectionalClass::Neutral => {}
        }
    }

    TextDirection::LeftToRight
}

fn build_directional_runs(
    text: &str,
    paragraph_direction: TextDirection,
    text_glyph_count: usize,
    glyphs: &[ShapedGlyph],
    output: &mut [DirectionalRun],
) -> Result<usize, ShapeError> {
    // without right-to-left text every glyph resolves to level 0, so the text is one run
    // however often words, numbers and punctuation alternate
    if !text
        .chars()
        .any(|character| directional_class(character) == DirectionalClass::RightToLeft)
    {
        let Some(destination) = output.first_mut() else {
            return Err(ShapeError::TooManyDirectionalRuns);
        };

        *destination = DirectionalRun::new(0, glyphs.len(), DirectionalClass::LeftToRight);

        return Ok(1);
    }

    let mut brackets = [BracketPair::EMPTY; BRACKET_PAIR_CAPACITY];
    let bracket_count = find_bracket_pairs(text, &mut brackets).unwrap_or(0);
    let brackets = &mut brackets[..bracket_count];

    resolve_bracket_pairs(text, paragraph_direction, brackets);

    let mut written = 0;

    for (index, glyph) in glyphs.iter().copied().enumerate() {
        let class = if index < text_glyph_count {
            directional_class_for_cluster(text, glyph.cluster(), brackets)
        } else {
            // extr glyphs, currently the renderer's ellipsis suffix, don't have cluster
            // offsets into `text`. Treat them as neutrals and let the paragraph context
            // place them
            DirectionalClass::Neutral
        };

        if written > 0 && output[written - 1].class == class {
            output[written - 1].end = index.saturating_add(1);
            continue;
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
            output[written - 2].end = index.saturating_add(1);
            written -= 1;
            continue;
        }

        let Some(desstination) = output.get_mut(written) else {
            return Err(ShapeError::TooManyDirectionalRuns);
        };

        *desstination = DirectionalRun::new(index, index.saturating_add(1), class);
        written = written.saturating_add(1);
    }

    Ok(written)
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

fn resolve_directional_run_levels(runs: &mut [DirectionalRun], paragraph_direction: TextDirection) {
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

fn mirror_odd_level_glyphs<const FONTS: usize>(
    registry: &FontRegistry<'_, FONTS>,
    size_px: u16,
    text: &str,
    text_glyph_count: usize,
    glyphs: &mut [ShapedGlyph],
    runs: &[DirectionalRun],
) {
    for run in runs.iter().copied() {
        if run.level % 2 == 0 {
            continue;
        }

        let start = run.start.min(text_glyph_count).min(glyphs.len());
        let end = run.end.min(text_glyph_count).min(glyphs.len());
        let mut previous_cluster = None;

        for glyph in &mut glyphs[start..end] {
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
            let Some(resolved) = registry.resolve_character_exact(shaped.font(), mirrored) else {
                continue;
            };

            let Some(base_advance) = resolved.face().glyph_advance(resolved.glyph(), size_px)
            else {
                continue;
            };

            *glyph = ShapedGlyph::new(
                resolved.font(),
                resolved.glyph(),
                shaped.cluster(),
                base_advance,
                Offset::ZERO,
                base_advance,
            );
        }
    }
}

fn mirrored_character(character: char) -> Option<char> {
    MIRRORING_GLYPHS.get(character).mirroring_glyph
}

fn previous_strong_direction(runs: &[DirectionalRun], index: usize) -> Option<TextDirection> {
    for run in runs[..index].iter().rev() {
        match run.class {
            DirectionalClass::LeftToRight => return Some(TextDirection::LeftToRight),
            DirectionalClass::RightToLeft => return Some(TextDirection::RightToLeft),
            DirectionalClass::Number | DirectionalClass::Neutral => {}
        }
    }

    None
}

fn next_strong_direction(runs: &[DirectionalRun], index: usize) -> Option<TextDirection> {
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
    runs: &[DirectionalRun],
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
    runs: &[DirectionalRun],
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

fn merge_same_level_runs(runs: &mut [DirectionalRun]) -> usize {
    if runs.is_empty() {
        return 0;
    }

    let mut written = 1usize;

    for read in 1..runs.len() {
        let run = runs[read];

        if runs[written - 1].level == run.level {
            runs[written - 1].end = run.end;
            continue;
        }

        runs[written] = run;
        written = written.saturating_add(1);
    }

    written
}

fn reorder_directional_runs(glyphs: &mut [ShapedGlyph], runs: &mut [DirectionalRun]) {
    let mut highest_level = 0u8;
    let mut lowest_odd_level: Option<u8> = None;

    for run in runs.iter().copied() {
        highest_level = highest_level.max(run.level);
        if run.level % 2 == 1 {
            lowest_odd_level = Some(match lowest_odd_level {
                Some(current) => current.min(run.level),
                None => run.level,
            });
        }
    }

    let Some(lowest_odd_level) = lowest_odd_level else {
        return;
    };

    let mut level = highest_level;

    loop {
        reverse_level_spans(glyphs, runs, level);
        if level == lowest_odd_level {
            break;
        }

        level = level.saturating_sub(1);
    }
}

fn reverse_level_spans(glyphs: &mut [ShapedGlyph], runs: &mut [DirectionalRun], level: u8) {
    let mut cursor = 0usize;

    while cursor < runs.len() {
        if runs[cursor].level < level {
            cursor = cursor.saturating_add(1);
            continue;
        }

        let start = cursor;

        while cursor < runs.len() && runs[cursor].level >= level {
            cursor = cursor.saturating_add(1);
        }

        reverse_directional_span(glyphs, &mut runs[start..cursor]);
    }
}

fn reverse_directional_span(glyphs: &mut [ShapedGlyph], runs: &mut [DirectionalRun]) {
    let Some(first) = runs.first().copied() else {
        return;
    };
    let Some(last) = runs.last().copied() else {
        return;
    };

    let start = first.start;
    let end = last.end;

    reverse_glyph_clusters(&mut glyphs[start..end]);
    runs.reverse();

    let mut cursor = start;

    for run in runs {
        let len = run.len();

        run.start = cursor;
        run.end = cursor.saturating_add(len);

        cursor = run.end;
    }

    debug_assert_eq!(cursor, end);
}

fn reverse_glyph_clusters(glyphs: &mut [ShapedGlyph]) {
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
