use core::{convert::Infallible, str::CharIndices};

use crate::{
    FontId, FontRegistry, GlyphId, Offset, Pixels, ResolvedGlyph, px,
    text_shaping::arabic::{contextual_form, joining_type, lam_alef_form},
};

mod arabic;

const DIRECTIONAL_RUN_CAPACITY: usize = 32;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum TextDirection {
    #[default]
    LeftToRight,
    RightToLeft,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
enum DirectionalClass {
    LeftToRight,
    RightToLeft,
    Number,
    Neutral,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DirectionalRun {
    start: usize,
    end: usize,
    class: DirectionalClass,
    context_direction: TextDirection,
    level: u8,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct ShapedGlyph {
    font: FontId,
    glyph: GlyphId,
    /// UTF-8 byte offset in the original input corresponding to the logical cluster that
    /// produced this glyph.
    ///
    /// multiple glyphs may later share one cluster, and one glyph represents multiple
    /// source characters
    cluster: usize,
    /// visual position relative to the current pen
    offset: Offset,
    /// amount by which the visual pen moves after this glyph
    advance: Pixels,
}

impl Default for ShapedGlyph {
    fn default() -> Self {
        Self::EMPTY
    }
}

impl ShapedGlyph {
    pub const EMPTY: Self = Self {
        font: FontId::DEFAULT,
        glyph: GlyphId::new(0),
        cluster: 0,
        offset: Offset::ZERO,
        advance: Pixels::ZERO,
    };

    pub const fn new(
        font: FontId,
        glyph: GlyphId,
        cluster: usize,
        offset: Offset,
        advance: Pixels,
    ) -> Self {
        Self {
            font,
            glyph,
            cluster,
            offset,
            advance,
        }
    }

    pub const fn font(self) -> FontId {
        self.font
    }

    pub const fn glyph(self) -> GlyphId {
        self.glyph
    }

    pub const fn cluster(self) -> usize {
        self.cluster
    }

    pub const fn offset(self) -> Offset {
        self.offset
    }

    pub const fn advance(self) -> Pixels {
        self.advance
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct ShapeSummary {
    glyph_count: usize,
    advance: Pixels,
}

impl ShapeSummary {
    pub const fn new(glyph_count: usize, advance: Pixels) -> Self {
        Self {
            glyph_count,
            advance,
        }
    }

    pub const fn glyph_count(self) -> usize {
        self.glyph_count
    }

    pub const fn advance(self) -> Pixels {
        self.advance
    }

    pub const fn is_empty(self) -> bool {
        self.glyph_count == 0
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct ShapeState {
    previous: Option<(FontId, GlyphId)>,
    /// logical joining state is intentionally separate from the previous rendered glyph.
    ///
    /// transparent arabic marks participate in the glyph stream but do not break
    /// contextual joining
    previous_joins_forward: bool,
}

impl ShapeState {
    pub const fn new() -> Self {
        Self {
            previous: None,
            previous_joins_forward: false,
        }
    }

    pub fn reset(&mut self) {
        self.previous = None;
        self.previous_joins_forward = false;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ShapeError {
    BufferTooSmall,
    TooManyDirectionalRuns,
}

pub struct ShapedRun<'a> {
    glyphs: &'a [ShapedGlyph],
    direction: TextDirection,
    advance: Pixels,
}

impl<'a> ShapedRun<'a> {
    pub const fn new(glyphs: &'a [ShapedGlyph], direction: TextDirection, advance: Pixels) -> Self {
        Self {
            glyphs,
            direction,
            advance,
        }
    }

    pub const fn glyphs(&self) -> &'a [ShapedGlyph] {
        self.glyphs
    }

    pub const fn direction(&self) -> TextDirection {
        self.direction
    }

    pub const fn advance(&self) -> Pixels {
        self.advance
    }

    pub const fn len(&self) -> usize {
        self.glyphs.len()
    }

    pub const fn is_empty(&self) -> bool {
        self.glyphs.is_empty()
    }
}

/// this is deliberately a very small shaper
///
/// it provides the architecture the renderer will use, but its current shaping rules
/// are only:
/// ```txt
///    Unicode scalar
///      -> font fallbakc
///      -> glyph
///      -> same-face kerning
/// ```
/// arabic joining/bidi will replace these rules without changing [`ShapedGlyph`] or
/// the renderer
#[derive(Debug, Default, Clone, Copy)]
pub struct SimpleShaper;

impl SimpleShaper {
    pub const fn new() -> Self {
        Self
    }

    pub fn try_shape_piece_with<'font, const FONTS: usize, F, E>(
        &self,
        registry: &FontRegistry<'font, FONTS>,
        preferred_font: FontId,
        size_px: u16,
        text: &str,
        state: &mut ShapeState,
        mut visit: F,
    ) -> Result<ShapeSummary, E>
    where
        F: FnMut(ShapedGlyph) -> Result<(), E>,
    {
        let mut advance = px(0);
        let mut glyph_count = 0usize;
        let mut characters = text.char_indices();
        let mut previous_cluster = None;

        while let Some((cluster, character)) = characters.next() {
            let joining = joining_type(character);

            // transparent marks belong to the preceeding logical cluster
            // they remain separate glyphs for now because we don't mark positioning yet,
            // but line breaking, ellipsis and bidi must treat the bae + marks as
            // invisible unit
            if joining.is_transparent() {
                let cluster = previous_cluster.unwrap_or(cluster);

                match registry.resolve_glyph(preferred_font, character) {
                    Some(resolved) => {
                        emit_resolved_glyph(
                            resolved,
                            cluster,
                            size_px,
                            state,
                            &mut advance,
                            &mut glyph_count,
                            &mut visit,
                        )?;
                    }
                    None => state.previous = None,
                }

                continue;
            }

            previous_cluster = Some(cluster);

            // mandatory lam-alef ligatures are deliberately conservative:
            //      lam + immediately adjacent alef
            // a transparent mark between them prevents ligation for now, matching the simple
            // FreeInk-style v1 behavior
            // only consume the alef when some registered font actualyl contains
            // the selected ligature glyph
            if character == '\u{0644}'
                && let Some((_alef_cluster, alef)) = characters.clone().next()
            {
                let joins_previous = state.previous_joins_forward && joining.accepts_previous();

                if let Some(ligature_character) = lam_alef_form(alef, joins_previous)
                    && let Some(resolved) =
                        registry.resolve_character_exact(preferred_font, ligature_character)
                {
                    // a cmap entry without usable horizontal metrics shouldn't
                    // make us consume two source characters
                    if resolved
                        .face()
                        .glyph_advance(resolved.glyph(), size_px)
                        .is_some()
                    {
                        let _ = characters.next();
                        emit_resolved_glyph(
                            resolved,
                            cluster,
                            size_px,
                            state,
                            &mut advance,
                            &mut glyph_count,
                            &mut visit,
                        )?;

                        // alef is right-joining only, so the resulting lam-alef
                        // ligature cannot continue forward
                        state.previous_joins_forward = false;
                        continue;
                    }
                }
            }

            let next_accepts = next_non_transparent_accepts_previous(&characters);
            let joins_previous = state.previous_joins_forward && joining.accepts_previous();
            let joins_next = joining.connects_forward() && next_accepts;
            let presentation = contextual_form(character, joins_previous, joins_next);
            let resolved =
                resolve_contextual_glyph(registry, preferred_font, character, presentation);

            match resolved {
                Some(resolved) => {
                    emit_resolved_glyph(
                        resolved,
                        cluster,
                        size_px,
                        state,
                        &mut advance,
                        &mut glyph_count,
                        &mut visit,
                    )?;
                }
                None => state.previous = None,
            }

            state.previous_joins_forward = joining.connects_forward();
        }

        Ok(ShapeSummary::new(glyph_count, advance))
    }

    pub fn shape_piece_with<'font, const FONTS: usize, F>(
        &self,
        registry: &FontRegistry<'font, FONTS>,
        preferred_font: FontId,
        size_px: u16,
        text: &str,
        state: &mut ShapeState,
        mut visit: F,
    ) -> ShapeSummary
    where
        F: FnMut(ShapedGlyph),
    {
        let result =
            self.try_shape_piece_with(registry, preferred_font, size_px, text, state, |glyph| {
                visit(glyph);
                Ok::<(), Infallible>(())
            });

        match result {
            Ok(summary) => summary,
            Err(error) => match error {},
        }
    }

    pub fn shape_piece_into<'font, const FONTS: usize>(
        &self,
        registry: &FontRegistry<'font, FONTS>,
        preferred_font: FontId,
        size_px: u16,
        text: &str,
        state: &mut ShapeState,
        output: &mut [ShapedGlyph],
    ) -> Result<ShapeSummary, ShapeError> {
        let mut written = 0usize;

        self.try_shape_piece_with(registry, preferred_font, size_px, text, state, |glyph| {
            let Some(destination) = output.get_mut(written) else {
                return Err(ShapeError::BufferTooSmall);
            };

            *destination = glyph;
            written = written.saturating_add(1);

            Ok(())
        })
    }

    pub fn measure<'font, const FONTS: usize>(
        &self,
        registry: &FontRegistry<'font, FONTS>,
        preferred_font: FontId,
        size_px: u16,
        text: &str,
    ) -> ShapeSummary {
        let mut state = ShapeState::new();
        self.shape_piece_with(registry, preferred_font, size_px, text, &mut state, |_| {})
    }

    pub fn next_cluster_boundary<'font, const FONTS: usize>(
        &self,
        registry: &FontRegistry<'font, FONTS>,
        preferred_font: FontId,
        size_px: u16,
        text: &str,
        from: usize,
    ) -> Option<usize> {
        if from >= text.len() || !text.is_char_boundary(from) {
            return None;
        }

        let mut state = ShapeState::new();
        let mut next_boundary: Option<usize> = None;

        self.shape_piece_with(
            registry,
            preferred_font,
            size_px,
            text,
            &mut state,
            |glyph| {
                let cluster = glyph.cluster();
                if cluster <= from {
                    return;
                }

                next_boundary = Some(match next_boundary {
                    Some(current) => current.min(cluster),
                    None => cluster,
                });
            },
        );

        next_boundary.or(Some(text.len()))
    }

    pub fn shape_into<'out, 'font, const FONTS: usize>(
        &self,
        registry: &FontRegistry<'font, FONTS>,
        preferred_font: FontId,
        size_px: u16,
        text: &str,
        output: &'out mut [ShapedGlyph],
    ) -> Result<ShapedRun<'out>, ShapeError> {
        let mut state = ShapeState::new();

        let summary =
            self.shape_piece_into(registry, preferred_font, size_px, text, &mut state, output)?;
        let glyph_count = summary.glyph_count();

        self.visual_order(
            text,
            glyph_count,
            &mut output[..glyph_count],
            summary.advance(),
        )
    }

    pub fn visual_order<'out>(
        &self,
        text: &str,
        text_glyph_count: usize,
        glyphs: &'out mut [ShapedGlyph],
        advance: Pixels,
    ) -> Result<ShapedRun<'out>, ShapeError> {
        let direction = paragraph_direction(text);
        if glyphs.len() <= 1 {
            return Ok(ShapedRun::new(glyphs, direction, advance));
        }

        let text_glyph_count = text_glyph_count.min(glyphs.len());
        let mut runs = [DirectionalRun::EMPTY; DIRECTIONAL_RUN_CAPACITY];

        let run_count = build_directional_runs(text, text_glyph_count, glyphs, &mut runs)?;
        resolve_directional_run_levels(&mut runs[..run_count], direction);

        let run_count = merge_same_level_runs(&mut runs[..run_count]);
        reorder_directional_runs(glyphs, &mut runs[..run_count]);

        Ok(ShapedRun::new(glyphs, direction, advance))
    }
}

fn next_non_transparent_accepts_previous(characters: &CharIndices<'_>) -> bool {
    let lookahead = characters.clone();

    for (_, character) in lookahead {
        let joining = joining_type(character);

        if joining.is_transparent() {
            continue;
        }

        return joining.accepts_previous();
    }

    false
}

fn resolve_contextual_glyph<'font, const FONTS: usize>(
    registry: &FontRegistry<'font, FONTS>,
    preferred_font: FontId,
    base_character: char,
    presentation: Option<char>,
) -> Option<ResolvedGlyph<'font>> {
    if let Some(presentation) = presentation
        && let Some(resolved) = registry.resolve_character_exact(preferred_font, presentation)
    {
        return Some(resolved);
    }

    // presentation Forms aren't mandatory in modern OpenType fonts.
    // if they aren't available, preserve readable base text rather than prematurely
    // returning a replacement character.
    registry.resolve_glyph(preferred_font, base_character)
}

fn emit_resolved_glyph<'font, F, E>(
    resolved: ResolvedGlyph<'font>,
    cluster: usize,
    size_px: u16,
    state: &mut ShapeState,
    advance: &mut Pixels,
    glyph_count: &mut usize,
    visit: &mut F,
) -> Result<bool, E>
where
    F: FnMut(ShapedGlyph) -> Result<(), E>,
{
    let font = resolved.font();
    let face = resolved.face();
    let glyph = resolved.glyph();

    let Some(base_advance) = face.glyph_advance(glyph, size_px) else {
        state.previous = None;
        return Ok(false);
    };

    let kerning = match state.previous {
        Some((previous_font, previous_glyph)) if previous_font == font => {
            face.kerning(previous_glyph, glyph, size_px)
        }
        _ => px(0),
    };

    let shaped = ShapedGlyph::new(
        font,
        glyph,
        cluster,
        Offset::new(kerning, px(0)),
        base_advance + kerning,
    );

    visit(shaped)?;

    *advance += shaped.advance();
    *glyph_count = glyph_count.saturating_add(1);
    state.previous = Some((font, glyph));

    Ok(true)
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
    if is_directional_number(character) {
        return DirectionalClass::Number;
    }
    if is_arabic_directional(character) && character.is_alphabetic() {
        return DirectionalClass::RightToLeft;
    }
    if character.is_alphabetic() {
        return DirectionalClass::LeftToRight;
    }

    DirectionalClass::Neutral
}

fn is_directional_number(character: char) -> bool {
    matches!(
        character,
        '0'..='9'
            | '\u{0660}'..='\u{0669}'
            | '\u{06F0}'..='\u{06F9}'
    )
}

fn is_arabic_directional(character: char) -> bool {
    matches!(
        character,
        '\u{0600}'..='\u{06FF}'
            | '\u{0750}'..='\u{077F}'
            | '\u{0870}'..='\u{089F}'
            | '\u{08A0}'..='\u{08FF}'
            | '\u{FB50}'..='\u{FDFF}'
            | '\u{FE70}'..='\u{FEFF}'
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    use crate::{FontFace, FontMetrics, FontRasterError, GlyphMetrics, px};

    struct TestFont {
        characters: &'static [char],
        advance: Pixels,
        kerning: Pixels,
    }

    impl FontFace for TestFont {
        fn glyph_id(&self, character: char) -> Option<GlyphId> {
            let index = self
                .characters
                .iter()
                .position(|candidate| *candidate == character)?;

            let index = u16::try_from(index).ok()?;

            Some(GlyphId::new(index.saturating_add(1)))
        }

        fn metrics(&self, _: u16) -> FontMetrics {
            FontMetrics::new(px(8), px(2), px(0))
        }

        fn glyph_metrics(&self, glyph: GlyphId, _: u16) -> Option<GlyphMetrics> {
            if glyph.value() == 0 {
                return None;
            }

            Some(GlyphMetrics::new(1, 1, px(0), px(-1), self.advance))
        }

        fn kerning(&self, _: GlyphId, _: GlyphId, _: u16) -> Pixels {
            self.kerning
        }

        fn rasterize(
            &self,
            _: GlyphId,
            _: u16,
            coverage: &mut [u8],
        ) -> Result<(), FontRasterError> {
            let Some(pixel) = coverage.first_mut() else {
                return Err(FontRasterError::BufferTooSmall);
            };

            *pixel = 255;

            Ok(())
        }
    }

    #[test]
    fn shaping_uses_font_fallback() {
        static LATIN: [char; 2] = ['A', '?'];
        static ARABIC: [char; 2] = ['ب', '?'];

        let latin = TestFont {
            characters: &LATIN,
            advance: px(5),
            kerning: px(0),
        };

        let arabic = TestFont {
            characters: &ARABIC,
            advance: px(7),
            kerning: px(0),
        };

        let mut registry = FontRegistry::<2>::default();
        let latin_id = registry.register(&latin).unwrap();
        let arabic_id = registry.register(&arabic).unwrap();
        let mut output = [ShapedGlyph::EMPTY; 2];

        let run = SimpleShaper::new()
            .shape_into(&registry, latin_id, 16, "Aب", &mut output)
            .unwrap();

        assert_eq!(run.len(), 2);
        assert_eq!(run.glyphs()[0].font(), latin_id);
        assert_eq!(run.glyphs()[1].font(), arabic_id);
        // "A" is one UTF-8 byte, so the Arabic codepoint begins at source byte offset 1.
        assert_eq!(run.glyphs()[1].cluster(), 1);
        assert_eq!(run.advance(), px(12));
    }

    #[test]
    fn kerning_is_not_applied_across_fallback_faces() {
        static LATIN: [char; 2] = ['A', '?'];
        static ARABIC: [char; 2] = ['ب', '?'];

        let latin = TestFont {
            characters: &LATIN,
            advance: px(5),
            kerning: px(-1),
        };

        let arabic = TestFont {
            characters: &ARABIC,
            advance: px(7),
            kerning: px(-2),
        };

        let mut registry = FontRegistry::<2>::default();
        let latin_id = registry.register(&latin).unwrap();

        registry.register(&arabic).unwrap();

        let shaper = SimpleShaper::new();

        assert_eq!(
            shaper.measure(&registry, latin_id, 16, "AA").advance(),
            px(9),
        );
        assert_eq!(
            shaper.measure(&registry, latin_id, 16, "Aب").advance(),
            px(12),
        );
    }

    #[test]
    fn missing_character_uses_replacement_fallback() {
        static CHARACTERS: [char; 2] = ['A', '?'];

        let font = TestFont {
            characters: &CHARACTERS,
            advance: px(5),
            kerning: px(0),
        };

        let mut registry = FontRegistry::<1>::default();
        let font_id = registry.register(&font).unwrap();
        let mut output = [ShapedGlyph::EMPTY; 1];
        let run = SimpleShaper::new()
            .shape_into(&registry, font_id, 16, "☃", &mut output)
            .unwrap();

        assert_eq!(run.len(), 1);
        assert_eq!(run.glyphs()[0].glyph(), font.glyph_id('?').unwrap());
    }

    #[test]
    fn shaped_run_uses_caller_owned_capacity() {
        static CHARACTERS: [char; 3] = ['A', 'B', '?'];

        let font = TestFont {
            characters: &CHARACTERS,
            advance: px(5),
            kerning: px(0),
        };

        let mut registry = FontRegistry::<1>::default();
        let font_id = registry.register(&font).unwrap();
        let mut output = [ShapedGlyph::EMPTY; 1];
        let result = SimpleShaper::new().shape_into(&registry, font_id, 16, "AB", &mut output);

        assert!(matches!(result, Err(ShapeError::BufferTooSmall)));
    }

    #[test]
    fn arabic_letters_select_contextual_forms() {
        static ARABIC: [char; 4] = [
            '\u{FE91}', // beh initial
            '\u{FE92}', // beh medial
            '\u{FE90}', // beh final
            '?',
        ];

        let arabic = TestFont {
            characters: &ARABIC,
            advance: px(7),
            kerning: px(0),
        };

        let mut registry = FontRegistry::<1>::default();
        let font = registry.register(&arabic).unwrap();
        let mut output = [ShapedGlyph::EMPTY; 3];

        let shaper = SimpleShaper::new();
        let measured = shaper.measure(&registry, font, 16, "ببب");

        let run = shaper
            .shape_into(&registry, font, 16, "ببب", &mut output)
            .unwrap();

        assert_eq!(run.direction(), TextDirection::RightToLeft);
        assert_eq!(run.len(), 3);

        // contextual shaping happens in logical order:
        //     initial, medial, final
        // but the returned run is in visual left-to-right storage order:
        //     final, medial, initial
        assert_eq!(run.glyphs()[0].glyph(), GlyphId::new(3));
        assert_eq!(run.glyphs()[1].glyph(), GlyphId::new(2));
        assert_eq!(run.glyphs()[2].glyph(), GlyphId::new(1));
        // logical UTF-8 source offsets stay attached to their glyphs.
        assert_eq!(run.glyphs()[0].cluster(), 4);
        assert_eq!(run.glyphs()[1].cluster(), 2);
        assert_eq!(run.glyphs()[2].cluster(), 0);
        // visual reordering must not change line width.
        assert_eq!(run.advance(), measured.advance());
        assert_eq!(run.advance(), px(21));
    }

    #[test]
    fn transparent_arabic_marks_do_not_break_joining_or_clusters() {
        static ARABIC: [char; 4] = [
            '\u{FE91}', // beh initial
            '\u{064E}', // fatha
            '\u{FE90}', // beh final
            '?',
        ];

        let arabic = TestFont {
            characters: &ARABIC,
            advance: px(7),
            kerning: px(0),
        };

        let mut registry = FontRegistry::<1>::default();
        let font = registry.register(&arabic).unwrap();
        let mut output = [ShapedGlyph::EMPTY; 3];
        let run = SimpleShaper::new()
            .shape_into(&registry, font, 16, "بَب", &mut output)
            .unwrap();

        assert_eq!(run.direction(), TextDirection::RightToLeft);
        assert_eq!(run.len(), 3);

        // logical shaping:
        //     beh(initial), fatha, beh(final)
        // the base + fatha share cluster 0. Visual RTL reordering moves complete clusters,
        // while preserving glyph order inside cluster 0.
        assert_eq!(run.glyphs()[0].glyph(), GlyphId::new(3));
        assert_eq!(run.glyphs()[1].glyph(), GlyphId::new(1));
        assert_eq!(run.glyphs()[2].glyph(), GlyphId::new(2));

        assert_eq!(run.glyphs()[0].cluster(), 4);
        assert_eq!(run.glyphs()[1].cluster(), 0);
        assert_eq!(run.glyphs()[2].cluster(), 0);
    }

    #[test]
    fn adjacent_lam_alef_uses_mandatory_ligature_when_font_has_it() {
        static ARABIC: [char; 2] = [
            '\u{FEFB}', // isolated lam-alef
            '?',
        ];

        let arabic = TestFont {
            characters: &ARABIC,
            advance: px(9),
            kerning: px(0),
        };

        let mut registry = FontRegistry::<1>::default();
        let font = registry.register(&arabic).unwrap();
        let mut output = [ShapedGlyph::EMPTY; 2];
        let run = SimpleShaper::new()
            .shape_into(&registry, font, 16, "لا", &mut output)
            .unwrap();

        assert_eq!(run.len(), 1);
        assert_eq!(run.glyphs()[0].glyph(), GlyphId::new(1));
        // one glyph represents the two-character source cluster and points at the original lam.
        assert_eq!(run.glyphs()[0].cluster(), 0);
        assert_eq!(run.advance(), px(9));
    }

    #[test]
    fn lam_alef_uses_final_ligature_when_connected_to_previous_letter() {
        static ARABIC: [char; 3] = [
            '\u{FE91}', // beh initial
            '\u{FEFC}', // final lam-alef
            '?',
        ];

        let arabic = TestFont {
            characters: &ARABIC,
            advance: px(8),
            kerning: px(0),
        };

        let mut registry = FontRegistry::<1>::default();
        let font = registry.register(&arabic).unwrap();
        let mut output = [ShapedGlyph::EMPTY; 3];
        let run = SimpleShaper::new()
            .shape_into(&registry, font, 16, "بلا", &mut output)
            .unwrap();

        assert_eq!(run.direction(), TextDirection::RightToLeft);
        assert_eq!(run.len(), 2);
        // the lam-alef is logically after beh, but appears first in the left-to-right
        // visual glyph buffer.
        assert_eq!(run.glyphs()[0].glyph(), GlyphId::new(2));
        assert_eq!(run.glyphs()[1].glyph(), GlyphId::new(1));
        // the ligature still points at the original logical lam.
        assert_eq!(run.glyphs()[0].cluster(), 2);
        assert_eq!(run.glyphs()[1].cluster(), 0);
    }

    #[test]
    fn lam_alef_is_not_consumed_when_ligature_is_missing() {
        static ARABIC: [char; 3] = [
            '\u{FEDF}', // lam initial
            '\u{FE8E}', // alef final
            '?',
        ];

        let arabic = TestFont {
            characters: &ARABIC,
            advance: px(7),
            kerning: px(0),
        };

        let mut registry = FontRegistry::<1>::default();
        let font = registry.register(&arabic).unwrap();
        let mut output = [ShapedGlyph::EMPTY; 2];
        let run = SimpleShaper::new()
            .shape_into(&registry, font, 16, "لا", &mut output)
            .unwrap();

        assert_eq!(run.direction(), TextDirection::RightToLeft);
        assert_eq!(run.len(), 2);
        // both source characters survive; only their visual order changes.
        assert_eq!(run.glyphs()[0].glyph(), GlyphId::new(2));
        assert_eq!(run.glyphs()[1].glyph(), GlyphId::new(1));
        assert_eq!(run.glyphs()[0].cluster(), 2);
        assert_eq!(run.glyphs()[1].cluster(), 0);
    }

    #[test]
    fn missing_presentation_form_falls_back_to_base_character() {
        static ARABIC: [char; 2] = ['ب', '?'];

        let arabic = TestFont {
            characters: &ARABIC,
            advance: px(7),
            kerning: px(0),
        };

        let mut registry = FontRegistry::<1>::default();
        let font = registry.register(&arabic).unwrap();
        let mut output = [ShapedGlyph::EMPTY; 2];
        let run = SimpleShaper::new()
            .shape_into(&registry, font, 16, "بب", &mut output)
            .unwrap();

        assert_eq!(run.len(), 2);
        assert_eq!(run.glyphs()[0].glyph(), GlyphId::new(1));
        assert_eq!(run.glyphs()[1].glyph(), GlyphId::new(1));
    }

    #[test]
    fn arabic_with_numbers_keeps_digits_in_ltr_order() {
        static CHARACTERS: [char; 5] = ['ب', ' ', '1', '2', '?'];

        let font = TestFont {
            characters: &CHARACTERS,
            advance: px(5),
            kerning: px(0),
        };

        let mut registry = FontRegistry::<1>::default();
        let font_id = registry.register(&font).unwrap();
        let mut output = [ShapedGlyph::EMPTY; 4];

        let shaper = SimpleShaper::new();
        let measured = shaper.measure(&registry, font_id, 16, "ب 12");

        let run = shaper
            .shape_into(&registry, font_id, 16, "ب 12", &mut output)
            .unwrap();

        assert_eq!(run.direction(), TextDirection::RightToLeft);
        assert_eq!(run.len(), 4);

        // logical clusters:
        //  ب = 0
        //   = 2
        // 1 = 3
        // 2 = 4
        //
        // visual storage is:
        // 1 2 <space> ب
        assert_eq!(run.glyphs()[0].cluster(), 3);
        assert_eq!(run.glyphs()[1].cluster(), 4);
        assert_eq!(run.glyphs()[2].cluster(), 2);
        assert_eq!(run.glyphs()[3].cluster(), 0);

        assert_eq!(run.advance(), measured.advance());
    }

    #[test]
    fn arabic_with_latin_preserves_ltr_run_order() {
        static CHARACTERS: [char; 5] = ['ب', ' ', 'A', 'B', '?'];

        let font = TestFont {
            characters: &CHARACTERS,
            advance: px(5),
            kerning: px(0),
        };

        let mut registry = FontRegistry::<1>::default();
        let font_id = registry.register(&font).unwrap();
        let mut output = [ShapedGlyph::EMPTY; 4];

        let shaper = SimpleShaper::new();
        let measured = shaper.measure(&registry, font_id, 16, "ب AB");

        let run = shaper
            .shape_into(&registry, font_id, 16, "ب AB", &mut output)
            .unwrap();

        assert_eq!(run.direction(), TextDirection::RightToLeft);

        // logical:
        // ب <space> A B
        //
        // visual:
        // A B <space> ب
        assert_eq!(run.glyphs()[0].cluster(), 3);
        assert_eq!(run.glyphs()[1].cluster(), 4);
        assert_eq!(run.glyphs()[2].cluster(), 2);
        assert_eq!(run.glyphs()[3].cluster(), 0);

        assert_eq!(run.advance(), measured.advance());
    }

    #[test]
    fn ltr_run_between_arabic_runs_keeps_internal_order() {
        static CHARACTERS: [char; 5] = ['ب', ' ', 'A', 'B', '?'];

        let font = TestFont {
            characters: &CHARACTERS,
            advance: px(5),
            kerning: px(0),
        };

        let mut registry = FontRegistry::<1>::default();
        let font_id = registry.register(&font).unwrap();
        let mut output = [ShapedGlyph::EMPTY; 6];

        let run = SimpleShaper::new()
            .shape_into(&registry, font_id, 16, "ب AB ب", &mut output)
            .unwrap();

        assert_eq!(run.direction(), TextDirection::RightToLeft);

        // logical clusters:
        // ب 0
        //   2
        // A 3
        // B 4
        //   5
        // ب 6
        //
        // visual:
        // ب <space> A B <space> ب
        // the Arabic runs exchange sides, while AB remains A,B.
        assert_eq!(run.glyphs()[0].cluster(), 6);
        assert_eq!(run.glyphs()[1].cluster(), 5);
        assert_eq!(run.glyphs()[2].cluster(), 3);
        assert_eq!(run.glyphs()[3].cluster(), 4);
        assert_eq!(run.glyphs()[4].cluster(), 2);
        assert_eq!(run.glyphs()[5].cluster(), 0);
    }

    #[test]
    fn bidi_reordering_preserves_resolved_fallback_fonts() {
        static LATIN: [char; 3] = ['A', ' ', '?'];
        static ARABIC: [char; 2] = ['ب', '?'];

        let latin = TestFont {
            characters: &LATIN,
            advance: px(5),
            kerning: px(0),
        };
        let arabic = TestFont {
            characters: &ARABIC,
            advance: px(7),
            kerning: px(0),
        };

        let mut registry = FontRegistry::<2>::default();

        let latin_id = registry.register(&latin).unwrap();
        let arabic_id = registry.register(&arabic).unwrap();

        let mut output = [ShapedGlyph::EMPTY; 3];

        let run = SimpleShaper::new()
            .shape_into(&registry, latin_id, 16, "ب A", &mut output)
            .unwrap();

        assert_eq!(run.direction(), TextDirection::RightToLeft);

        assert_eq!(run.glyphs()[0].cluster(), 3);
        assert_eq!(run.glyphs()[0].font(), latin_id);

        assert_eq!(run.glyphs()[1].cluster(), 2);
        assert_eq!(run.glyphs()[1].font(), latin_id);

        assert_eq!(run.glyphs()[2].cluster(), 0);
        assert_eq!(run.glyphs()[2].font(), arabic_id);
    }

    #[test]
    fn cluster_boundary_keeps_transparent_mark_with_base() {
        static ARABIC: [char; 4] = [
            '\u{FE91}', // beh initial
            '\u{064E}', // fatha
            '\u{FE90}', // beh final
            '?',
        ];

        let arabic = TestFont {
            characters: &ARABIC,
            advance: px(7),
            kerning: px(0),
        };

        let mut registry = FontRegistry::<1>::default();
        let font = registry.register(&arabic).unwrap();

        let shaper = SimpleShaper::new();
        let text = "بَب";

        // UTF-8:
        //     ب   0..2
        //     َ   2..4
        //     ب   4..6
        // the first legal break is after base + mark.
        assert_eq!(
            shaper.next_cluster_boundary(&registry, font, 16, text, 0),
            Some(4),
        );
        assert_eq!(
            shaper.next_cluster_boundary(&registry, font, 16, text, 4),
            Some(6),
        );
        assert_eq!(
            shaper.next_cluster_boundary(&registry, font, 16, text, 6),
            None,
        );
    }

    #[test]
    fn cluster_boundary_keeps_lam_alef_ligature_together() {
        static ARABIC: [char; 2] = [
            '\u{FEFB}', // isolated lam-alef
            '?',
        ];

        let arabic = TestFont {
            characters: &ARABIC,
            advance: px(9),
            kerning: px(0),
        };

        let mut registry = FontRegistry::<1>::default();
        let font = registry.register(&arabic).unwrap();

        let shaper = SimpleShaper::new();
        let text = "لا";

        assert_eq!(
            shaper.next_cluster_boundary(&registry, font, 16, text, 0,),
            Some(text.len()),
        );
        assert_eq!(
            shaper.next_cluster_boundary(&registry, font, 16, text, text.len(),),
            None,
        );
    }
}
