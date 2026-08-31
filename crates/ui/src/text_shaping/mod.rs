use core::{convert::Infallible, str::CharIndices};

use crate::{
    FontId, FontRegistry, GlyphId, GlyphMetrics, Offset, Pixels, ResolvedGlyph, px,
    text_shaping::arabic::{
        MarkPlacement, contextual_form, joining_type, lam_alef_form, mark_placement,
    },
};

mod arabic;

const DIRECTIONAL_RUN_CAPACITY: usize = 32;
const MARK_GAP: Pixels = px(1);

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
    /// multiple glyphs may share one cluster, and one glyph represents multiple
    /// source characters
    cluster: usize,
    /// unpositioned horizontal advance reported by the font.
    ///
    /// this deliberately remains separate from `advance` so bidi reordering can discard
    /// logical-order and rebuild final visual positioning without recovering the value
    /// from `offset`
    base_advance: Pixels,
    /// number of logical source components represented by this base glyph.
    ///
    /// normal bases contain one component. Marks contain zero. Our current lam-alef
    /// ligature contains 2
    ligature_components: u16,
    /// arabic combining-mark role
    mark_placement: Option<MarkPlacement>,
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
        base_advance: Pixels::ZERO,
        ligature_components: 0,
        mark_placement: None,
        offset: Offset::ZERO,
        advance: Pixels::ZERO,
    };

    pub const fn new(
        font: FontId,
        glyph: GlyphId,
        cluster: usize,
        base_advance: Pixels,
        offset: Offset,
        advance: Pixels,
    ) -> Self {
        Self {
            font,
            glyph,
            cluster,
            base_advance,
            ligature_components: 1,
            mark_placement: None,
            offset,
            advance,
        }
    }

    const fn new_ligature(
        font: FontId,
        glyph: GlyphId,
        cluster: usize,
        base_advance: Pixels,
        ligature_components: u16,
        offset: Offset,
        advance: Pixels,
    ) -> Self {
        Self {
            font,
            glyph,
            cluster,
            base_advance,
            ligature_components,
            mark_placement: None,
            offset,
            advance,
        }
    }

    const fn new_mark(
        font: FontId,
        glyph: GlyphId,
        cluster: usize,
        base_advance: Pixels,
        mark_placement: MarkPlacement,
        offset: Offset,
    ) -> Self {
        Self {
            font,
            glyph,
            cluster,
            base_advance,
            ligature_components: 0,
            mark_placement: Some(mark_placement),
            offset,
            advance: Pixels::ZERO,
        }
    }

    const fn with_positioning(self, offset: Offset, advance: Pixels) -> Self {
        Self {
            font: self.font,
            glyph: self.glyph,
            cluster: self.cluster,
            base_advance: self.base_advance,
            ligature_components: self.ligature_components,
            mark_placement: self.mark_placement,
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

    pub const fn base_advance(self) -> Pixels {
        self.base_advance
    }

    const fn final_ligature_component(self) -> Option<u16> {
        if self.ligature_components > 1 {
            Some(self.ligature_components - 1)
        } else {
            None
        }
    }

    const fn mark_placement(self) -> Option<MarkPlacement> {
        self.mark_placement
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
    /// logical joining state is intentionally separate from the previous rendered glyph.
    ///
    /// transparent arabic marks participate in the glyph stream but do not break
    /// contextual joining
    previous_joins_forward: bool,
}

impl ShapeState {
    pub const fn new() -> Self {
        Self {
            previous_joins_forward: false,
        }
    }

    pub fn reset(&mut self) {
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

                if let Some(resolved) = registry.resolve_glyph(preferred_font, character) {
                    if let Some(placement) = mark_placement(character) {
                        emit_resolved_mark(
                            resolved,
                            cluster,
                            size_px,
                            placement,
                            &mut glyph_count,
                            &mut visit,
                        )?;
                    } else {
                        // Preserve the existing behavior for transparent
                        // characters outside our supported Arabic-mark
                        // subset. We can expand the table deliberately
                        // later rather than guessing their placement.
                        emit_resolved_glyph(
                            resolved,
                            cluster,
                            size_px,
                            &mut advance,
                            &mut glyph_count,
                            &mut visit,
                        )?;
                    }
                }

                continue;
            }

            previous_cluster = Some(cluster);

            // the only ligature produced by the simple shaper today is adjacent lam + alef.
            if character == '\u{0644}'
                && let Some((_alef_cluster, alef)) = characters.clone().next()
            {
                let joins_previous = state.previous_joins_forward && joining.accepts_previous();

                if let Some(ligature_character) = lam_alef_form(alef, joins_previous)
                    && let Some(resolved) =
                        registry.resolve_character_exact(preferred_font, ligature_character)
                    && resolved
                        .face()
                        .glyph_advance(resolved.glyph(), size_px)
                        .is_some()
                {
                    let _ = characters.next();

                    emit_resolved_ligature(
                        resolved,
                        cluster,
                        size_px,
                        2,
                        &mut advance,
                        &mut glyph_count,
                        &mut visit,
                    )?;

                    state.previous_joins_forward = false;
                    continue;
                }
            }

            let next_accepts = next_non_transparent_accepts_previous(&characters);
            let joins_previous = state.previous_joins_forward && joining.accepts_previous();
            let joins_next = joining.connects_forward() && next_accepts;
            let presentation = contextual_form(character, joins_previous, joins_next);
            let resolved =
                resolve_contextual_glyph(registry, preferred_font, character, presentation);

            if let Some(resolved) = resolved {
                emit_resolved_glyph(
                    resolved,
                    cluster,
                    size_px,
                    &mut advance,
                    &mut glyph_count,
                    &mut visit,
                )?;
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
        output: &mut [ShapedGlyph],
    ) -> Result<ShapeSummary, ShapeError> {
        let run = self.shape_into(registry, preferred_font, size_px, text, output)?;

        Ok(ShapeSummary::new(run.len(), run.advance()))
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
            registry,
            size_px,
            text,
            glyph_count,
            &mut output[..glyph_count],
        )
    }

    pub fn visual_order<'out, 'font, const FONTS: usize>(
        &self,
        registry: &FontRegistry<'font, FONTS>,
        size_px: u16,
        text: &str,
        text_glyph_count: usize,
        glyphs: &'out mut [ShapedGlyph],
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

        let advance = apply_visual_positioning(registry, size_px, glyphs);

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

fn emit_resolved_mark<'font, F, E>(
    resolved: ResolvedGlyph<'font>,
    cluster: usize,
    size_px: u16,
    placement: MarkPlacement,
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
        return Ok(false);
    };

    let shaped = ShapedGlyph::new_mark(font, glyph, cluster, base_advance, placement, Offset::ZERO);

    visit(shaped)?;

    // a combining mark exists in the glyph stream and counts toward buffer capacity,
    // but does not move the pen.
    *glyph_count = glyph_count.saturating_add(1);

    Ok(true)
}

fn emit_resolved_ligature<'font, F, E>(
    resolved: ResolvedGlyph<'font>,
    cluster: usize,
    size_px: u16,
    ligature_components: u16,
    advance: &mut Pixels,
    glyph_count: &mut usize,
    visit: &mut F,
) -> Result<bool, E>
where
    F: FnMut(ShapedGlyph) -> Result<(), E>,
{
    debug_assert!(ligature_components > 1);

    let font = resolved.font();
    let face = resolved.face();
    let glyph = resolved.glyph();

    let Some(base_advance) = face.glyph_advance(glyph, size_px) else {
        return Ok(false);
    };

    let shaped = ShapedGlyph::new_ligature(
        font,
        glyph,
        cluster,
        base_advance,
        ligature_components,
        Offset::ZERO,
        base_advance,
    );

    visit(shaped)?;

    *advance += base_advance;
    *glyph_count = glyph_count.saturating_add(1);

    Ok(true)
}

fn emit_resolved_glyph<'font, F, E>(
    resolved: ResolvedGlyph<'font>,
    cluster: usize,
    size_px: u16,
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
        return Ok(false);
    };

    // logical shaping produces unpositioned glyphs.
    // pair positioning belongs to the final visual glyph stream, after bidi ordering
    // and mirroring are complete.
    let shaped = ShapedGlyph::new(
        font,
        glyph,
        cluster,
        base_advance,
        Offset::ZERO,
        base_advance,
    );

    visit(shaped)?;

    *advance += base_advance;
    *glyph_count = glyph_count.saturating_add(1);

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
    match character {
        '(' => Some(')'),
        ')' => Some('('),

        '[' => Some(']'),
        ']' => Some('['),

        '{' => Some('}'),
        '}' => Some('{'),

        '<' => Some('>'),
        '>' => Some('<'),

        '«' => Some('»'),
        '»' => Some('«'),

        '‹' => Some('›'),
        '›' => Some('‹'),

        '⁅' => Some('⁆'),
        '⁆' => Some('⁅'),

        '〈' => Some('〉'),
        '〉' => Some('〈'),

        '⟨' => Some('⟩'),
        '⟩' => Some('⟨'),

        '⟦' => Some('⟧'),
        '⟧' => Some('⟦'),

        '⟪' => Some('⟫'),
        '⟫' => Some('⟪'),

        _ => None,
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

fn apply_visual_positioning<'font, const FONTS: usize>(
    registry: &FontRegistry<'font, FONTS>,
    size_px: u16,
    glyphs: &mut [ShapedGlyph],
) -> Pixels {
    let mut previous_base: Option<ShapedGlyph> = None;
    let mut visual_advance = px(0);
    let mut index = 0usize;

    while index < glyphs.len() {
        let shaped = glyphs[index];

        // a mark without a base can occur at the beginning of malformed or intentionally
        // isolated input. Keep it non-spacing and leave it at its natural baseline origin.
        if let Some(placement) = shaped.mark_placement() {
            glyphs[index] = ShapedGlyph::new_mark(
                shaped.font(),
                shaped.glyph(),
                shaped.cluster(),
                shaped.base_advance(),
                placement,
                Offset::ZERO,
            );

            index = index.saturating_add(1);
            continue;
        }

        let kerning = match previous_base {
            Some(previous) if previous.font() == shaped.font() => registry
                .get(shaped.font())
                .map(|face| face.kerning(previous.glyph(), shaped.glyph(), size_px))
                .unwrap_or(px(0)),

            _ => px(0),
        };

        let positioned_base =
            shaped.with_positioning(Offset::new(kerning, px(0)), shaped.base_advance() + kerning);

        glyphs[index] = positioned_base;
        visual_advance += positioned_base.advance();

        let mut mark_end = index.saturating_add(1);

        while mark_end < glyphs.len() {
            let candidate = glyphs[mark_end];
            if candidate.cluster() != positioned_base.cluster()
                || candidate.mark_placement().is_none()
            {
                break;
            }

            mark_end = mark_end.saturating_add(1);
        }

        position_cluster_marks(
            registry,
            size_px,
            positioned_base,
            &mut glyphs[index.saturating_add(1)..mark_end],
        );

        previous_base = Some(positioned_base);
        index = mark_end;
    }

    visual_advance
}

fn position_cluster_marks<'font, const FONTS: usize>(
    registry: &FontRegistry<'font, FONTS>,
    size_px: u16,
    base: ShapedGlyph,
    marks: &mut [ShapedGlyph],
) {
    if marks.is_empty() {
        return;
    }
    if position_cluster_marks_with_font_anchors(registry, size_px, base, marks) {
        return;
    }

    position_cluster_marks_with_metrics(registry, size_px, base, marks);
}

fn position_cluster_marks_with_font_anchors<'font, const FONTS: usize>(
    registry: &FontRegistry<'font, FONTS>,
    size_px: u16,
    base: ShapedGlyph,
    marks: &mut [ShapedGlyph],
) -> bool {
    let mut previous_mark: Option<ShapedGlyph> = None;

    for mark in marks.iter_mut() {
        let shaped = *mark;
        let Some(placement) = shaped.mark_placement() else {
            return false;
        };

        let mut attachment = None;

        // prefer mark-to-mark when another mark in this cluster has already been positioned.
        if let Some(parent) = previous_mark
            && parent.font() == shaped.font()
        {
            attachment = registry
                .get(shaped.font())
                .and_then(|face| face.mark_to_mark_offset(parent.glyph(), shaped.glyph(), size_px))
                .map(|offset| {
                    Offset::new(parent.offset().x + offset.x, parent.offset().y + offset.y)
                });
        }

        // our current ligature producer is lam-alef.
        // a mark encountered after the consumed alef belongs to the final logical
        // component of that ligature.
        if attachment.is_none()
            && base.font() == shaped.font()
            && let Some(component) = base.final_ligature_component()
        {
            attachment = registry
                .get(shaped.font())
                .and_then(|face| {
                    face.mark_to_ligature_offset(base.glyph(), component, shaped.glyph(), size_px)
                })
                .map(|offset| Offset::new(offset.x - base.base_advance(), offset.y));
        }

        // ordinary single-component base.
        if attachment.is_none() && base.font() == shaped.font() {
            attachment = registry
                .get(shaped.font())
                .and_then(|face| face.mark_to_base_offset(base.glyph(), shaped.glyph(), size_px))
                .map(|offset| Offset::new(offset.x - base.base_advance(), offset.y));
        }

        let Some(offset) = attachment else {
            // keep the cluster internally consistent:
            // if any mark cannot use font anchors, the caller will replace the whole
            // cluster with metric positioning.
            return false;
        };

        let positioned = ShapedGlyph::new_mark(
            shaped.font(),
            shaped.glyph(),
            shaped.cluster(),
            shaped.base_advance(),
            placement,
            offset,
        );

        *mark = positioned;

        previous_mark = Some(positioned);
    }

    true
}

fn position_cluster_marks_with_metrics<'font, const FONTS: usize>(
    registry: &FontRegistry<'font, FONTS>,
    size_px: u16,
    base: ShapedGlyph,
    marks: &mut [ShapedGlyph],
) {
    let fallback_offset = Offset::new(Pixels::ZERO - base.base_advance(), Pixels::ZERO);

    let Some(base_metrics) = registry
        .get(base.font())
        .and_then(|face| face.glyph_metrics(base.glyph(), size_px))
    else {
        for mark in marks {
            let shaped = *mark;

            let Some(placement) = shaped.mark_placement() else {
                continue;
            };

            *mark = ShapedGlyph::new_mark(
                shaped.font(),
                shaped.glyph(),
                shaped.cluster(),
                shaped.base_advance(),
                placement,
                fallback_offset,
            );
        }

        return;
    };

    let base_height = px(i32::from(base_metrics.height));
    let mut above_edge = base_metrics.bearing_y;
    let mut below_edge = base_metrics.bearing_y + base_height;

    // shadda stays nearest to the base even when canonical Unicode ordering places
    // another above-base mark first.
    for target in [
        MarkPlacement::Shadda,
        MarkPlacement::Above,
        MarkPlacement::Below,
    ] {
        for mark in marks.iter_mut() {
            let shaped = *mark;
            if shaped.mark_placement() != Some(target) {
                continue;
            }

            let mark_metrics = registry
                .get(shaped.font())
                .and_then(|face| face.glyph_metrics(shaped.glyph(), size_px));

            let offset = match mark_metrics {
                Some(mark_metrics) => {
                    let edge = match target {
                        MarkPlacement::Shadda | MarkPlacement::Above => &mut above_edge,
                        MarkPlacement::Below => &mut below_edge,
                    };

                    positioned_mark_offset(base, base_metrics, mark_metrics, target, edge)
                }

                None => fallback_offset,
            };

            *mark = ShapedGlyph::new_mark(
                shaped.font(),
                shaped.glyph(),
                shaped.cluster(),
                shaped.base_advance(),
                target,
                offset,
            );
        }
    }
}

fn positioned_mark_offset(
    base: ShapedGlyph,
    base_metrics: GlyphMetrics,
    mark_metrics: GlyphMetrics,
    placement: MarkPlacement,
    edge: &mut Pixels,
) -> Offset {
    let base_width = px(i32::from(base_metrics.width));
    let mark_width = px(i32::from(mark_metrics.width));
    let mark_height = px(i32::from(mark_metrics.height));

    // the renderer reaches the mark after it has already advanced past the base. Move back
    // by the base's intrinsic advance, then center the mark over the base's actual ink bounds.
    // pair kerning on the base cancels naturally because it shifts both the drawn base
    // and the post-base pen by the same amount.
    let x = base_metrics.bearing_x + (base_width - mark_width) / 2
        - mark_metrics.bearing_x
        - base.base_advance();

    let y = match placement {
        MarkPlacement::Shadda | MarkPlacement::Above => {
            let bottom = *edge - MARK_GAP;
            let top = bottom - mark_height;

            *edge = top;

            top - mark_metrics.bearing_y
        }

        MarkPlacement::Below => {
            let top = *edge + MARK_GAP;

            *edge = top + mark_height;

            top - mark_metrics.bearing_y
        }
    };

    Offset::new(x, y)
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

    struct AsymmetricKerningFont {
        characters: &'static [char],
        advance: Pixels,
    }

    impl FontFace for AsymmetricKerningFont {
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

        fn kerning(&self, left: GlyphId, right: GlyphId, _: u16) -> Pixels {
            match (left.value(), right.value()) {
                // logical initial -> final
                (1, 2) => px(-1),
                // visual final -> initial
                (2, 1) => px(-3),
                _ => px(0),
            }
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

    struct AnchoredMarkFont {
        characters: &'static [char],
        advance: Pixels,
    }

    impl FontFace for AnchoredMarkFont {
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

        fn mark_to_base_offset(&self, base: GlyphId, mark: GlyphId, _: u16) -> Option<Offset> {
            match (base.value(), mark.value()) {
                // isolated beh -> fatha
                (1, 2) => Some(Offset::new(px(2), px(-3))),

                _ => None,
            }
        }

        fn mark_to_mark_offset(&self, base_mark: GlyphId, mark: GlyphId, _: u16) -> Option<Offset> {
            match (base_mark.value(), mark.value()) {
                // fatha -> shadda
                (2, 3) => Some(Offset::new(px(0), px(-2))),

                _ => None,
            }
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

    struct LigatureAnchorFont {
        characters: &'static [char],
        advance: Pixels,
    }

    impl FontFace for LigatureAnchorFont {
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

        fn mark_to_ligature_offset(
            &self,
            ligature: GlyphId,
            component: u16,
            mark: GlyphId,
            _: u16,
        ) -> Option<Offset> {
            match (ligature.value(), component, mark.value()) {
                // isolated lam-alef, second logical component, fatha.
                (1, 1, 2) => Some(Offset::new(px(3), px(-4))),
                _ => None,
            }
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
        let mut latin_output = [ShapedGlyph::EMPTY; 2];
        let mut mixed_output = [ShapedGlyph::EMPTY; 2];

        assert_eq!(
            shaper
                .measure(&registry, latin_id, 16, "AA", &mut latin_output)
                .unwrap()
                .advance(),
            px(9),
        );
        assert_eq!(
            shaper
                .measure(&registry, latin_id, 16, "Aب", &mut mixed_output)
                .unwrap()
                .advance(),
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
        let mut measured_output = [ShapedGlyph::EMPTY; 3];
        let mut output = [ShapedGlyph::EMPTY; 3];

        let shaper = SimpleShaper::new();

        let measured = shaper
            .measure(&registry, font, 16, "ببب", &mut measured_output)
            .unwrap();

        let run = shaper
            .shape_into(&registry, font, 16, "ببب", &mut output)
            .unwrap();

        assert_eq!(run.direction(), TextDirection::RightToLeft);
        assert_eq!(run.len(), 3);
        assert_eq!(run.glyphs()[0].glyph(), GlyphId::new(3));
        assert_eq!(run.glyphs()[1].glyph(), GlyphId::new(2));
        assert_eq!(run.glyphs()[2].glyph(), GlyphId::new(1));
        assert_eq!(run.glyphs()[0].cluster(), 4);
        assert_eq!(run.glyphs()[1].cluster(), 2);
        assert_eq!(run.glyphs()[2].cluster(), 0);
        assert_eq!(run.advance(), measured.advance());
        assert_eq!(run.advance(), px(21));
    }

    #[test]
    fn transparent_arabic_marks_do_not_break_joining_or_advance() {
        static ARABIC: [char; 4] = [
            '\u{FE91}', // beh initial
            '\u{064E}', // fatha
            '\u{FE90}', // beh final
            '?',
        ];

        let arabic = TestFont {
            characters: &ARABIC,
            advance: px(7),
            kerning: px(-1),
        };

        let mut registry = FontRegistry::<1>::default();
        let arabic_id = registry.register(&arabic).unwrap();
        let shaper = SimpleShaper::new();
        let mut measured_output = [ShapedGlyph::EMPTY; 3];

        let measured = shaper
            .measure(&registry, arabic_id, 16, "بَب", &mut measured_output)
            .unwrap();

        let mut output = [ShapedGlyph::EMPTY; 3];

        let run = shaper
            .shape_into(&registry, arabic_id, 16, "بَب", &mut output)
            .unwrap();

        assert_eq!(run.direction(), TextDirection::RightToLeft);
        assert_eq!(run.len(), 3);

        // visual RTL order:
        // final beh
        // initial beh
        // fatha attached to initial beh
        assert_eq!(
            run.glyphs()[0].glyph(),
            arabic.glyph_id('\u{FE90}').unwrap(),
        );
        assert_eq!(
            run.glyphs()[1].glyph(),
            arabic.glyph_id('\u{FE91}').unwrap(),
        );
        assert_eq!(
            run.glyphs()[2].glyph(),
            arabic.glyph_id('\u{064E}').unwrap(),
        );
        assert_eq!(run.glyphs()[0].cluster(), 4);
        assert_eq!(run.glyphs()[1].cluster(), 0);
        assert_eq!(run.glyphs()[2].cluster(), 0);
        // kerning still applies directly between the two visual bases.
        assert_eq!(run.glyphs()[1].offset().x, px(-1));
        // TestFont has a 1x1 bitmap with bearing (0, -1).
        // the fatha moves back over the base and one pixel above it.
        assert_eq!(run.glyphs()[2].offset().x, px(-7));

        assert_eq!(run.glyphs()[2].offset().y, px(-2));

        // the font itself reports an advance of 7 for every test glyph, but a combining
        // mark must not consume horizontal space.
        assert_eq!(run.glyphs()[2].base_advance(), px(7));
        assert_eq!(run.glyphs()[2].advance(), px(0));
        // two bases, with -1 visual kerning:
        //     7 + (7 - 1) = 13
        assert_eq!(run.advance(), px(13));
        assert_eq!(measured.advance(), run.advance());
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

        let shaper = SimpleShaper::new();

        let mut measured_output = [ShapedGlyph::EMPTY; 4];
        let measured = shaper
            .measure(&registry, font_id, 16, "ب 12", &mut measured_output)
            .unwrap();

        let mut output = [ShapedGlyph::EMPTY; 4];
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

        let shaper = SimpleShaper::new();

        let mut measured_output = [ShapedGlyph::EMPTY; 4];
        let measured = shaper
            .measure(&registry, font_id, 16, "ب AB", &mut measured_output)
            .unwrap();

        let mut output = [ShapedGlyph::EMPTY; 4];
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
            shaper.next_cluster_boundary(&registry, font, 16, text, 0),
            Some(text.len()),
        );
        assert_eq!(
            shaper.next_cluster_boundary(&registry, font, 16, text, text.len()),
            None,
        );
    }

    #[test]
    fn rtl_brackets_are_mirrored_around_ltr_run() {
        static CHARACTERS: [char; 7] = ['ب', ' ', '(', ')', 'A', 'B', '?'];

        let font = TestFont {
            characters: &CHARACTERS,
            advance: px(5),
            kerning: px(0),
        };

        let mut registry = FontRegistry::<1>::default();

        let font_id = registry.register(&font).unwrap();

        let mut output = [ShapedGlyph::EMPTY; 6];

        let run = SimpleShaper::new()
            .shape_into(&registry, font_id, 16, "ب (AB)", &mut output)
            .unwrap();

        assert_eq!(run.direction(), TextDirection::RightToLeft);

        // logical UTF-8 clusters:
        //     ب  0
        //        2
        //     (  3
        //     A  4
        //     B  5
        //     )  6
        //
        // visual:
        //     ( A B ) <space> ب
        //
        // the LTR run stays A,B and the bracket glyphs are mirrored.
        assert_eq!(run.glyphs()[0].cluster(), 6);
        assert_eq!(run.glyphs()[1].cluster(), 4);
        assert_eq!(run.glyphs()[2].cluster(), 5);
        assert_eq!(run.glyphs()[3].cluster(), 3);
        assert_eq!(run.glyphs()[4].cluster(), 2);
        assert_eq!(run.glyphs()[5].cluster(), 0);

        assert_eq!(run.glyphs()[0].glyph(), font.glyph_id('(').unwrap());

        assert_eq!(run.glyphs()[3].glyph(), font.glyph_id(')').unwrap());

        assert_eq!(run.advance(), px(30));
    }

    #[test]
    fn ltr_brackets_around_rtl_run_are_not_mirrored() {
        static CHARACTERS: [char; 7] = ['A', 'B', ' ', '(', ')', 'ب', '?'];

        let font = TestFont {
            characters: &CHARACTERS,
            advance: px(5),
            kerning: px(0),
        };

        let mut registry = FontRegistry::<1>::default();

        let font_id = registry.register(&font).unwrap();

        let mut output = [ShapedGlyph::EMPTY; 6];

        let run = SimpleShaper::new()
            .shape_into(&registry, font_id, 16, "AB (ب)", &mut output)
            .unwrap();

        assert_eq!(run.direction(), TextDirection::LeftToRight);

        assert_eq!(run.glyphs()[0].cluster(), 0);
        assert_eq!(run.glyphs()[1].cluster(), 1);
        assert_eq!(run.glyphs()[2].cluster(), 2);
        assert_eq!(run.glyphs()[3].cluster(), 3);
        assert_eq!(run.glyphs()[4].cluster(), 4);
        assert_eq!(run.glyphs()[5].cluster(), 6);

        assert_eq!(run.glyphs()[3].glyph(), font.glyph_id('(').unwrap());

        assert_eq!(run.glyphs()[5].glyph(), font.glyph_id(')').unwrap());
    }

    #[test]
    fn mirrored_brackets_can_resolve_through_font_fallback() {
        static PRIMARY: [char; 6] = ['ب', ' ', '(', 'A', 'B', '?'];
        static FALLBACK: [char; 2] = [')', '?'];

        let primary = TestFont {
            characters: &PRIMARY,
            advance: px(5),
            kerning: px(0),
        };
        let fallback = TestFont {
            characters: &FALLBACK,
            advance: px(7),
            kerning: px(0),
        };

        let mut registry = FontRegistry::<2>::default();
        let primary_id = registry.register(&primary).unwrap();
        let fallback_id = registry.register(&fallback).unwrap();
        let mut output = [ShapedGlyph::EMPTY; 6];

        let run = SimpleShaper::new()
            .shape_into(&registry, primary_id, 16, "ب (AB)", &mut output)
            .unwrap();

        // the logical closing ')' originally came from the fallback face.
        // after RTL mirroring it becomes '(' and resolves back to PRIMARY.
        assert_eq!(run.glyphs()[0].glyph(), primary.glyph_id('(').unwrap());
        assert_eq!(run.glyphs()[0].font(), primary_id);
        // the logical opening '(' becomes ')' and therefore resolves through
        // the registered fallback face.
        assert_eq!(run.glyphs()[3].glyph(), fallback.glyph_id(')').unwrap());
        assert_eq!(run.glyphs()[3].font(), fallback_id);
        // mirroring does not alter measured line width.
        assert_eq!(run.advance(), px(32));
    }

    #[test]
    fn rtl_kerning_is_measured_in_visual_order() {
        static ARABIC: [char; 3] = [
            '\u{FE91}', // beh initial
            '\u{FE90}', // beh final
            '?',
        ];

        let font = AsymmetricKerningFont {
            characters: &ARABIC,
            advance: px(5),
        };

        let mut registry = FontRegistry::<1>::default();
        let font_id = registry.register(&font).unwrap();
        let shaper = SimpleShaper::new();

        let mut measured_output = [ShapedGlyph::EMPTY; 2];

        let measured = shaper
            .measure(&registry, font_id, 16, "بب", &mut measured_output)
            .unwrap();

        // logical glyph order:
        //     initial -> final
        // visual glyph order:
        //     final -> initial
        // the visual pair has kerning -3, so:
        //     5 + (5 - 3) = 7
        assert_eq!(measured.advance(), px(7));

        let mut output = [ShapedGlyph::EMPTY; 2];

        let run = shaper
            .shape_into(&registry, font_id, 16, "بب", &mut output)
            .unwrap();

        assert_eq!(run.direction(), TextDirection::RightToLeft);

        // visual order is final -> initial.
        assert_eq!(run.glyphs()[0].glyph(), GlyphId::new(2));
        assert_eq!(run.glyphs()[1].glyph(), GlyphId::new(1));
        // the first visual glyph has no preceding pair.
        assert_eq!(run.glyphs()[0].offset().x, px(0));
        // kerning is now computed from the actual visual pair:
        //     final -> initial = -3
        assert_eq!(run.glyphs()[1].offset().x, px(-3));
        assert_eq!(run.glyphs()[0].advance(), px(5));
        assert_eq!(run.glyphs()[1].advance(), px(2));
        assert_eq!(run.advance(), px(7));
        assert_eq!(run.advance(), measured.advance());
        assert_eq!(
            run.glyphs()[0].advance() + run.glyphs()[1].advance(),
            run.advance(),
        );
    }

    #[test]
    fn ltr_kerning_stays_on_the_second_visual_glyph() {
        static CHARACTERS: [char; 2] = ['A', '?'];

        let font = TestFont {
            characters: &CHARACTERS,
            advance: px(5),
            kerning: px(-1),
        };

        let mut registry = FontRegistry::<1>::default();
        let font_id = registry.register(&font).unwrap();
        let mut output = [ShapedGlyph::EMPTY; 2];
        let run = SimpleShaper::new()
            .shape_into(&registry, font_id, 16, "AA", &mut output)
            .unwrap();

        assert_eq!(run.direction(), TextDirection::LeftToRight);
        assert_eq!(run.glyphs()[0].offset().x, px(0));
        assert_eq!(run.glyphs()[1].offset().x, px(-1));
        assert_eq!(run.glyphs()[0].advance(), px(5));
        assert_eq!(run.glyphs()[1].advance(), px(4));
        assert_eq!(run.advance(), px(9));
    }

    #[test]
    fn arabic_marks_stack_around_their_base() {
        static ARABIC: [char; 5] = [
            '\u{FE8F}', // beh isolated
            '\u{064E}', // fatha
            '\u{0651}', // shadda
            '\u{0650}', // kasra
            '?',
        ];

        let arabic = TestFont {
            characters: &ARABIC,
            advance: px(5),
            kerning: px(0),
        };

        let mut registry = FontRegistry::<1>::default();

        let arabic_id = registry.register(&arabic).unwrap();

        let mut output = [ShapedGlyph::EMPTY; 4];

        let run = SimpleShaper::new()
            .shape_into(
                &registry,
                arabic_id,
                16,
                concat!(
                    "\u{0628}", // beh
                    "\u{064E}", // fatha
                    "\u{0651}", // shadda
                    "\u{0650}", // kasra
                ),
                &mut output,
            )
            .unwrap();

        assert_eq!(run.direction(), TextDirection::RightToLeft);
        assert_eq!(run.len(), 4);
        assert_eq!(
            run.glyphs()[0].glyph(),
            arabic.glyph_id('\u{FE8F}').unwrap(),
        );
        // source order is fatha then shadda.
        assert_eq!(
            run.glyphs()[1].glyph(),
            arabic.glyph_id('\u{064E}').unwrap(),
        );
        assert_eq!(
            run.glyphs()[2].glyph(),
            arabic.glyph_id('\u{0651}').unwrap(),
        );
        assert_eq!(
            run.glyphs()[3].glyph(),
            arabic.glyph_id('\u{0650}').unwrap(),
        );

        for glyph in run.glyphs() {
            assert_eq!(glyph.cluster(), 0);
        }

        // all marks return to the base's horizontal position.
        assert_eq!(run.glyphs()[1].offset().x, px(-5));
        assert_eq!(run.glyphs()[2].offset().x, px(-5));
        assert_eq!(run.glyphs()[3].offset().x, px(-5));
        // shadda is deliberately closest to the base even though fatha occurs first
        // in the source string.
        assert_eq!(run.glyphs()[2].offset().y, px(-2));
        assert_eq!(run.glyphs()[1].offset().y, px(-4));
        // kasra is independently stacked below.
        assert_eq!(run.glyphs()[3].offset().y, px(2));
        assert_eq!(run.glyphs()[1].advance(), px(0));
        assert_eq!(run.glyphs()[2].advance(), px(0));
        assert_eq!(run.glyphs()[3].advance(), px(0));
        assert_eq!(run.advance(), px(5));
    }

    #[test]
    fn font_anchors_position_arabic_mark_chain() {
        static ARABIC: [char; 4] = [
            '\u{FE8F}', // beh isolated
            '\u{064E}', // fatha
            '\u{0651}', // shadda
            '?',
        ];

        let arabic = AnchoredMarkFont {
            characters: &ARABIC,
            advance: px(5),
        };
        let mut registry = FontRegistry::<1>::default();
        let font_id = registry.register(&arabic).unwrap();
        let mut output = [ShapedGlyph::EMPTY; 3];
        let run = SimpleShaper::new()
            .shape_into(
                &registry,
                font_id,
                16,
                concat!(
                    "\u{0628}", // beh
                    "\u{064E}", // fatha
                    "\u{0651}", // shadda
                ),
                &mut output,
            )
            .unwrap();

        assert_eq!(run.direction(), TextDirection::RightToLeft);
        assert_eq!(run.len(), 3);

        let base = run.glyphs()[0];
        let fatha = run.glyphs()[1];
        let shadda = run.glyphs()[2];

        assert_eq!(base.glyph(), arabic.glyph_id('\u{FE8F}').unwrap());
        assert_eq!(fatha.glyph(), arabic.glyph_id('\u{064E}').unwrap());
        assert_eq!(shadda.glyph(), arabic.glyph_id('\u{0651}').unwrap());
        assert_eq!(base.cluster(), 0);
        assert_eq!(fatha.cluster(), 0);
        assert_eq!(shadda.cluster(), 0);

        // font gives the fatha an attachment origin of (2, -3) relative to the base.
        // the renderer is already 5px past the base when it draws the mark:
        //     2 - 5 = -3
        assert_eq!(fatha.offset(), Offset::new(px(-3), px(-3)));
        // shadda then attaches directly to fatha:
        //     (-3, -3) + (0, -2) = (-3, -5)
        assert_eq!(shadda.offset(), Offset::new(px(-3), px(-5)));
        assert_eq!(fatha.advance(), px(0));
        assert_eq!(shadda.advance(), px(0));
        // marks never contribute to horizontal width.
        assert_eq!(run.advance(), px(5));
    }

    #[test]
    fn mark_after_lam_alef_uses_ligature_component_anchor() {
        static ARABIC: [char; 3] = [
            '\u{FEFB}', // isolated lam-alef
            '\u{064E}', // fatha
            '?',
        ];

        let arabic = LigatureAnchorFont {
            characters: &ARABIC,
            advance: px(8),
        };

        let mut registry = FontRegistry::<1>::default();
        let font_id = registry.register(&arabic).unwrap();
        let mut output = [ShapedGlyph::EMPTY; 2];
        let run = SimpleShaper::new()
            .shape_into(
                &registry,
                font_id,
                16,
                concat!(
                    "\u{0644}", // lam
                    "\u{0627}", // alef
                    "\u{064E}", // fatha
                ),
                &mut output,
            )
            .unwrap();

        assert_eq!(run.direction(), TextDirection::RightToLeft);
        assert_eq!(run.len(), 2);

        let ligature = run.glyphs()[0];
        let fatha = run.glyphs()[1];

        assert_eq!(ligature.glyph(), arabic.glyph_id('\u{FEFB}').unwrap());
        assert_eq!(fatha.glyph(), arabic.glyph_id('\u{064E}').unwrap());
        assert_eq!(ligature.cluster(), 0);
        assert_eq!(fatha.cluster(), 0);
        // the lam-alef base remembers that it represents two logical source components.
        assert_eq!(ligature.ligature_components, 2);
        // only component 1 has an anchor in LigatureAnchorFont.
        // font anchor:
        //     (3, -4)
        // renderer pen after the 8px ligature:
        //     x = 3 - 8 = -5
        assert_eq!(fatha.offset(), Offset::new(px(-5), px(-4)));
        assert_eq!(fatha.advance(), px(0));
        assert_eq!(run.advance(), px(8));
    }
}
