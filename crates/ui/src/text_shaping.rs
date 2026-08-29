use core::convert::Infallible;

use crate::{FontId, FontRegistry, GlyphId, Offset, Pixels, px};

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
}

impl ShapeState {
    pub const fn new() -> Self {
        Self { previous: None }
    }

    pub fn reset(&mut self) {
        self.previous = None;
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

        for (cluster, character) in text.char_indices() {
            let Some(resolved) = registry.resolve_glyph(preferred_font, character) else {
                // don't kern across an omitted character
                state.previous = None;
                continue;
            };

            let font = resolved.font();
            let face = resolved.face();
            let glyph = resolved.glyph();

            let Some(base_advance) = face.glyph_advance(glyph, size_px) else {
                state.previous = None;
                continue;
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

            advance += shaped.advance();
            glyph_count = glyph_count.saturating_add(1);
            state.previous = Some((font, glyph));
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

        assert_eq!(run.len(), 2,);
        assert_eq!(run.glyphs()[0].font(), latin_id,);
        assert_eq!(run.glyphs()[1].font(), arabic_id,);
        // "A" is one UTF-8 byte, so the Arabic codepoint begins at source byte offset 1.
        assert_eq!(run.glyphs()[1].cluster(), 1,);
        assert_eq!(run.advance(), px(12),);
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
            shaper.measure(&registry, latin_id, 16, "AA",).advance(),
            px(9),
        );
        assert_eq!(
            shaper.measure(&registry, latin_id, 16, "Aب",).advance(),
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

        assert_eq!(run.len(), 1,);
        assert_eq!(run.glyphs()[0].glyph(), font.glyph_id('?',).unwrap(),);
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

        assert!(matches!(result, Err(ShapeError::BufferTooSmall,),));
    }
}
