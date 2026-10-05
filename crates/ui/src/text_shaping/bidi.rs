use icu_properties::{
    CodePointMapData, CodePointMapDataBorrowed,
    props::{BidiClass, BidiMirroringGlyph},
};

use crate::{FontRegistry, Offset, PreparedFont, pair_cache::PairPositioningCache, px};

use super::{
    ShapeError, ShapedGlyph, ShapedRun, SimpleShaper, TextDirection,
    positioning::apply_visual_positioning,
};

const DIRECTIONAL_RUN_CAPACITY: usize = 32;

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

impl SimpleShaper {
    pub fn visual_order<'out, 'font, const FONTS: usize>(
        &self,
        registry: &FontRegistry<'font, FONTS>,
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
        registry: &FontRegistry<'font, FONTS>,
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

    pub(crate) fn visual_order_with_pair_positioning_cache<
        'out,
        'font,
        const FONTS: usize,
        const SLOTS: usize,
    >(
        &self,
        registry: &FontRegistry<'font, FONTS>,
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

    fn visual_order_impl<'out, 'font, const FONTS: usize, const SLOTS: usize>(
        &self,
        registry: &FontRegistry<'font, FONTS>,
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

        let run_count = build_directional_runs(text, text_glyph_count, glyphs, &mut runs)?;

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

    let mut written = 0;

    for (index, glyph) in glyphs.iter().copied().enumerate() {
        let class = if index < text_glyph_count {
            directional_class_for_cluster(text, glyph.cluster())
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

fn directional_class_for_cluster(text: &str, cluster: usize) -> DirectionalClass {
    let Some(remaining) = text.get(cluster..) else {
        return DirectionalClass::Neutral;
    };
    let Some(character) = remaining.chars().next() else {
        return DirectionalClass::Neutral;
    };

    directional_class(character)
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

fn mirror_odd_level_glyphs<'font, const FONTS: usize>(
    registry: &FontRegistry<'font, FONTS>,
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
