use core::{convert::Infallible, str::CharIndices};

use crate::{
    FontId, FontRegistry, GlyphId, Offset, Pixels, ResolvedGlyph, px,
    text_shaping::arabic::{contextual_form, joining_type, lam_alef_form},
};

mod arabic;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum TextDirection {
    #[default]
    LeftToRight,
    RightToLeft,
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

        while let Some((cluster, character)) = characters.next() {
            let joining = joining_type(character);

            // transparent marks remain ordinary output glyphs, but they must not modify
            // the logical joining chain.
            if joining.is_transparent() {
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

    pub fn shape_into<'out, 'font, const FONTS: usize>(
        &self,
        registry: &FontRegistry<'font, FONTS>,
        preferred_font: FontId,
        size_px: u16,
        text: &str,
        output: &'out mut [ShapedGlyph],
    ) -> Result<ShapedRun<'out>, ShapeError> {
        let mut state = ShapeState::new();
        let mut written = 0;

        let summary = self.try_shape_piece_with(
            registry,
            preferred_font,
            size_px,
            text,
            &mut state,
            |glyph| {
                let Some(destination) = output.get_mut(written) else {
                    return Err(ShapeError::BufferTooSmall);
                };

                *destination = glyph;
                written = written.saturating_add(1);

                Ok(())
            },
        )?;

        Ok(ShapedRun::new(
            &output[..written],
            TextDirection::LeftToRight,
            summary.advance(),
        ))
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
        let run = SimpleShaper::new()
            .shape_into(&registry, font, 16, "ببب", &mut output)
            .unwrap();

        assert_eq!(run.len(), 3);
        assert_eq!(run.glyphs()[0].glyph(), GlyphId::new(1));
        assert_eq!(run.glyphs()[1].glyph(), GlyphId::new(2));
        assert_eq!(run.glyphs()[2].glyph(), GlyphId::new(3));
        // arabic UTF-8 scalars here are two bytes each.
        assert_eq!(run.glyphs()[0].cluster(), 0);
        assert_eq!(run.glyphs()[1].cluster(), 2);
        assert_eq!(run.glyphs()[2].cluster(), 4);
    }

    #[test]
    fn transparent_arabic_marks_do_not_break_joining() {
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

        assert_eq!(run.len(), 3);
        assert_eq!(run.glyphs()[0].glyph(), GlyphId::new(1));
        assert_eq!(run.glyphs()[1].glyph(), GlyphId::new(2));
        assert_eq!(run.glyphs()[2].glyph(), GlyphId::new(3));
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

        assert_eq!(run.len(), 2);
        assert_eq!(run.glyphs()[0].glyph(), GlyphId::new(1));
        assert_eq!(run.glyphs()[1].glyph(), GlyphId::new(2));
        // beh begins at byte 0; lam begins at byte 2.
        assert_eq!(run.glyphs()[1].cluster(), 2);
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

        assert_eq!(run.len(), 2);
        assert_eq!(run.glyphs()[0].glyph(), GlyphId::new(1));
        assert_eq!(run.glyphs()[1].glyph(), GlyphId::new(2));
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
}
