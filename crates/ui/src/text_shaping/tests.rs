use super::*;

use crate::{
    FontFace, FontMetrics, FontRasterError, FontWeight, FontWeightRange, GlyphMetrics, px,
    text_shaping::logical::{
        ARABIC_FINA_FEATURE, ARABIC_INIT_FEATURE, ARABIC_ISOL_FEATURE, ARABIC_MEDI_FEATURE,
        ARABIC_RLIG_FEATURE,
    },
};

/// a line from the streaming shaper, collected so tests can index its glyphs
#[derive(Debug, PartialEq)]
struct ShapedLine {
    glyphs: std::vec::Vec<ShapedGlyph>,
    direction: TextDirection,
    advance: Pixels,
}

impl ShapedLine {
    fn glyphs(&self) -> &[ShapedGlyph] {
        &self.glyphs
    }

    fn direction(&self) -> TextDirection {
        self.direction
    }

    fn advance(&self) -> Pixels {
        self.advance
    }

    fn len(&self) -> usize {
        self.glyphs.len()
    }
}

trait ShapeLine {
    fn shape_line<const FONTS: usize>(
        &self,
        registry: &FontRegistry<'_, FONTS>,
        font: FontId,
        size_px: u16,
        text: &str,
    ) -> Result<ShapedLine, ShapeError>;

    /// shapes without keeping the glyphs
    fn measure_line<const FONTS: usize>(
        &self,
        registry: &FontRegistry<'_, FONTS>,
        font: FontId,
        size_px: u16,
        text: &str,
    ) -> Result<ShapeSummary, ShapeError>;
}

impl ShapeLine for SimpleShaper {
    fn shape_line<const FONTS: usize>(
        &self,
        registry: &FontRegistry<'_, FONTS>,
        font: FontId,
        size_px: u16,
        text: &str,
    ) -> Result<ShapedLine, ShapeError> {
        let mut glyphs = std::vec::Vec::new();
        let summary =
            self.prepare(registry, font)
                .shape_line_with(registry, size_px, text, "", |glyph| glyphs.push(glyph))?;

        assert_eq!(summary.glyph_count(), glyphs.len());

        Ok(ShapedLine {
            glyphs,
            direction: paragraph_direction(text),
            advance: summary.advance(),
        })
    }

    fn measure_line<const FONTS: usize>(
        &self,
        registry: &FontRegistry<'_, FONTS>,
        font: FontId,
        size_px: u16,
        text: &str,
    ) -> Result<ShapeSummary, ShapeError> {
        self.prepare(registry, font)
            .shape_line_with(registry, size_px, text, "", |_| {})
    }
}

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

    fn rasterize(&self, _: GlyphId, _: u16, coverage: &mut [u8]) -> Result<(), FontRasterError> {
        let Some(pixel) = coverage.first_mut() else {
            return Err(FontRasterError::BufferTooSmall);
        };

        *pixel = 255;

        Ok(())
    }
}

/// a font whose glyphs each have their own ink box, given as
/// (character, width, height, bearing x, bearing y)
struct BoxFont {
    glyphs: &'static [(char, u16, u16, i32, i32)],
    advance: Pixels,
}

impl FontFace for BoxFont {
    fn glyph_id(&self, character: char) -> Option<GlyphId> {
        let index = self
            .glyphs
            .iter()
            .position(|(candidate, ..)| *candidate == character)?;

        let index = u16::try_from(index).ok()?;

        Some(GlyphId::new(index.saturating_add(1)))
    }

    fn metrics(&self, _: u16) -> FontMetrics {
        FontMetrics::new(px(8), px(2), px(0))
    }

    fn glyph_metrics(&self, glyph: GlyphId, _: u16) -> Option<GlyphMetrics> {
        let index = usize::from(glyph.value()).checked_sub(1)?;
        let (_, width, height, bearing_x, bearing_y) = *self.glyphs.get(index)?;

        Some(GlyphMetrics::new(
            width,
            height,
            px(bearing_x),
            px(bearing_y),
            self.advance,
        ))
    }

    fn rasterize(&self, _: GlyphId, _: u16, coverage: &mut [u8]) -> Result<(), FontRasterError> {
        coverage.fill(255);

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

    fn rasterize(&self, _: GlyphId, _: u16, coverage: &mut [u8]) -> Result<(), FontRasterError> {
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

    fn rasterize(&self, _: GlyphId, _: u16, coverage: &mut [u8]) -> Result<(), FontRasterError> {
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

    fn rasterize(&self, _: GlyphId, _: u16, coverage: &mut [u8]) -> Result<(), FontRasterError> {
        let Some(pixel) = coverage.first_mut() else {
            return Err(FontRasterError::BufferTooSmall);
        };

        *pixel = 255;

        Ok(())
    }
}

struct CursiveTestFont {
    characters: &'static [char],
    advance: Pixels,
}

impl FontFace for CursiveTestFont {
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
        // deliberately large enough that the regression fails if cursive attachment
        // does not takeprecedence.
        px(-5)
    }

    fn cursive_attachment(
        &self,
        visual_left: GlyphId,
        visual_right: GlyphId,
        _: u16,
        right_to_left: bool,
    ) -> Option<crate::CursiveAttachment> {
        if !right_to_left {
            return None;
        }

        match (visual_left.value(), visual_right.value()) {
            // final beh -> medial beh
            (1, 2) => Some(crate::CursiveAttachment::new(Offset::new(px(4), px(-2)))),
            // medial beh -> initial beh
            (2, 4) => Some(crate::CursiveAttachment::new(Offset::new(px(3), px(1)))),
            _ => None,
        }
    }

    fn mark_to_base_offset(&self, base: GlyphId, mark: GlyphId, _: u16) -> Option<Offset> {
        match (base.value(), mark.value()) {
            // medial beh -> fatha
            (2, 3) => Some(Offset::new(px(1), px(-3))),

            _ => None,
        }
    }

    fn rasterize(&self, _: GlyphId, _: u16, coverage: &mut [u8]) -> Result<(), FontRasterError> {
        let Some(pixel) = coverage.first_mut() else {
            return Err(FontRasterError::BufferTooSmall);
        };

        *pixel = 255;

        Ok(())
    }
}

struct GsubArabicFont {
    characters: &'static [char],
    advance: Pixels,
}

impl FontFace for GsubArabicFont {
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

    fn single_substitution(
        &self,
        feature: crate::OpenTypeFeature,
        glyph: GlyphId,
    ) -> Option<GlyphId> {
        match glyph.value() {
            // first cmap glyph is either beh or lam, depending on the test.
            1 if feature == ARABIC_ISOL_FEATURE => Some(GlyphId::new(104)),
            1 if feature == ARABIC_INIT_FEATURE => Some(GlyphId::new(101)),
            1 if feature == ARABIC_MEDI_FEATURE => Some(GlyphId::new(102)),
            1 if feature == ARABIC_FINA_FEATURE => Some(GlyphId::new(103)),
            // second cmap glyph in the lam-alef test is alef.
            2 if feature == ARABIC_FINA_FEATURE => Some(GlyphId::new(203)),
            _ => None,
        }
    }

    fn ligature_substitution(
        &self,
        feature: crate::OpenTypeFeature,
        first: GlyphId,
        second: GlyphId,
    ) -> Option<GlyphId> {
        if feature != ARABIC_RLIG_FEATURE {
            return None;
        }

        match (first.value(), second.value()) {
            // contextual lam initial + contextual alef final
            (101, 203) => Some(GlyphId::new(300)),

            _ => None,
        }
    }

    fn rasterize(&self, _: GlyphId, _: u16, coverage: &mut [u8]) -> Result<(), FontRasterError> {
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
    let run = SimpleShaper::new()
        .shape_line(&registry, latin_id, 16, "Aب")
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
        shaper
            .measure_line(&registry, latin_id, 16, "AA")
            .unwrap()
            .advance(),
        px(9),
    );
    assert_eq!(
        shaper
            .measure_line(&registry, latin_id, 16, "Aب")
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
    let run = SimpleShaper::new()
        .shape_line(&registry, font_id, 16, "☃")
        .unwrap();

    assert_eq!(run.len(), 1);
    assert_eq!(run.glyphs()[0].glyph(), font.glyph_id('?').unwrap());
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

    let shaper = SimpleShaper::new();

    let measured = shaper.measure_line(&registry, font, 16, "ببب").unwrap();

    let run = shaper.shape_line(&registry, font, 16, "ببب").unwrap();

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
fn default_ignorables_draw_nothing() {
    // the font has a soft hyphen glyph, which must still not be drawn
    static CHARACTERS: [char; 4] = ['a', 'b', '\u{00AD}', '?'];

    let font = TestFont {
        characters: &CHARACTERS,
        advance: px(5),
        kerning: px(0),
    };

    let mut registry = FontRegistry::<1>::default();
    let font_id = registry.register(&font).unwrap();
    let shaper = SimpleShaper::new();

    // soft hyphen, zero-width space, zero-width joiner, variation selector 16
    let text = "a\u{00AD}\u{200B}\u{200D}\u{FE0F}b";

    let measured = shaper.measure_line(&registry, font_id, 16, text).unwrap();

    let run = shaper.shape_line(&registry, font_id, 16, text).unwrap();

    assert_eq!(run.len(), 2);
    assert_eq!(run.glyphs()[0].glyph(), font.glyph_id('a').unwrap());
    assert_eq!(run.glyphs()[1].glyph(), font.glyph_id('b').unwrap());
    assert_eq!(run.glyphs()[1].cluster(), text.len() - 1);
    assert_eq!(run.advance(), px(10));
    assert_eq!(run.advance(), measured.advance());
}

#[test]
fn zero_width_non_joiner_breaks_arabic_joining() {
    static ARABIC: [char; 2] = [
        '\u{FE8F}', // beh isolated
        '?',
    ];

    let arabic = TestFont {
        characters: &ARABIC,
        advance: px(7),
        kerning: px(0),
    };

    let mut registry = FontRegistry::<1>::default();
    let font = registry.register(&arabic).unwrap();
    let run = SimpleShaper::new()
        .shape_line(&registry, font, 16, "ب\u{200C}ب")
        .unwrap();

    assert_eq!(run.len(), 2);
    assert_eq!(run.glyphs()[0].glyph(), GlyphId::new(1));
    assert_eq!(run.glyphs()[1].glyph(), GlyphId::new(1));
    assert_eq!(run.advance(), px(14));
}

#[test]
fn zero_width_joiner_forces_arabic_joining() {
    static ARABIC: [char; 3] = [
        '\u{FE91}', // beh initial
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
    let run = SimpleShaper::new()
        .shape_line(&registry, font, 16, "ب\u{200D}ب")
        .unwrap();

    // visual RTL order: final beh, then initial beh. The joiner draws nothing
    assert_eq!(run.len(), 2);
    assert_eq!(
        run.glyphs()[0].glyph(),
        arabic.glyph_id('\u{FE90}').unwrap()
    );
    assert_eq!(
        run.glyphs()[1].glyph(),
        arabic.glyph_id('\u{FE91}').unwrap()
    );
    assert_eq!(run.advance(), px(14));
}

#[test]
fn arabic_extended_a_letters_join() {
    static ARABIC: [char; 4] = [
        '\u{FE91}', // beh initial
        '\u{08A0}', // beh with small v below
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
    let run = SimpleShaper::new()
        .shape_line(&registry, font, 16, "ب\u{08A0}ب")
        .unwrap();

    // U+08A0 has no presentation forms, so its neighbours show the joins
    assert_eq!(run.len(), 3);
    assert_eq!(
        run.glyphs()[0].glyph(),
        arabic.glyph_id('\u{FE90}').unwrap()
    );
    assert_eq!(
        run.glyphs()[1].glyph(),
        arabic.glyph_id('\u{08A0}').unwrap()
    );
    assert_eq!(
        run.glyphs()[2].glyph(),
        arabic.glyph_id('\u{FE91}').unwrap()
    );
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
    let measured = shaper.measure_line(&registry, arabic_id, 16, "بَب").unwrap();

    let run = shaper.shape_line(&registry, arabic_id, 16, "بَب").unwrap();

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
    let run = SimpleShaper::new()
        .shape_line(&registry, font, 16, "لا")
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
    let run = SimpleShaper::new()
        .shape_line(&registry, font, 16, "بلا")
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
    let run = SimpleShaper::new()
        .shape_line(&registry, font, 16, "لا")
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
    let run = SimpleShaper::new()
        .shape_line(&registry, font, 16, "بب")
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

    let measured = shaper.measure_line(&registry, font_id, 16, "ب 12").unwrap();

    let run = shaper.shape_line(&registry, font_id, 16, "ب 12").unwrap();

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
fn long_ltr_line_is_not_limited_by_word_count() {
    static CHARACTERS: [char; 6] = ['a', ' ', '1', ',', '"', '?'];

    let font = TestFont {
        characters: &CHARACTERS,
        advance: px(5),
        kerning: px(0),
    };

    let mut registry = FontRegistry::<1>::default();
    let font_id = registry.register(&font).unwrap();

    // every word, number and punctuation mark switches directional class
    let text = "\"a, 1 ".repeat(20);
    let run = SimpleShaper::new()
        .shape_line(&registry, font_id, 16, &text)
        .unwrap();

    assert_eq!(run.direction(), TextDirection::LeftToRight);
    assert_eq!(run.len(), text.len());
    assert!(
        run.glyphs()
            .windows(2)
            .all(|pair| pair[0].cluster() < pair[1].cluster())
    );
}

#[test]
fn long_rtl_line_is_not_limited_by_word_count() {
    static CHARACTERS: [char; 3] = ['ب', ' ', '?'];

    let font = TestFont {
        characters: &CHARACTERS,
        advance: px(5),
        kerning: px(0),
    };

    let mut registry = FontRegistry::<1>::default();
    let font_id = registry.register(&font).unwrap();

    let text = "ب ".repeat(40);
    let run = SimpleShaper::new()
        .shape_line(&registry, font_id, 16, text.trim_end())
        .unwrap();

    assert_eq!(run.direction(), TextDirection::RightToLeft);
    assert_eq!(run.len(), 79);
    assert!(
        run.glyphs()
            .windows(2)
            .all(|pair| pair[0].cluster() > pair[1].cluster())
    );
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

    let measured = shaper.measure_line(&registry, font_id, 16, "ب AB").unwrap();

    let run = shaper.shape_line(&registry, font_id, 16, "ب AB").unwrap();

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
    let run = SimpleShaper::new()
        .shape_line(&registry, font_id, 16, "ب AB ب")
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
fn hebrew_resolves_right_to_left() {
    static CHARACTERS: [char; 5] = ['א', 'ב', ' ', 'A', '?'];

    let font = TestFont {
        characters: &CHARACTERS,
        advance: px(5),
        kerning: px(0),
    };

    let mut registry = FontRegistry::<1>::default();
    let font_id = registry.register(&font).unwrap();
    let run = SimpleShaper::new()
        .shape_line(&registry, font_id, 16, "אב A")
        .unwrap();

    assert_eq!(run.direction(), TextDirection::RightToLeft);

    // logical clusters:
    // א 0
    // ב 2
    //   4
    // A 5
    //
    // visual:
    // A <space> ב א
    assert_eq!(run.glyphs()[0].cluster(), 5);
    assert_eq!(run.glyphs()[1].cluster(), 4);
    assert_eq!(run.glyphs()[2].cluster(), 2);
    assert_eq!(run.glyphs()[3].cluster(), 0);
}

#[test]
fn hebrew_run_inside_ltr_paragraph_is_reversed() {
    static CHARACTERS: [char; 5] = ['א', 'ב', ' ', 'A', '?'];

    let font = TestFont {
        characters: &CHARACTERS,
        advance: px(5),
        kerning: px(0),
    };

    let mut registry = FontRegistry::<1>::default();
    let font_id = registry.register(&font).unwrap();
    let run = SimpleShaper::new()
        .shape_line(&registry, font_id, 16, "A אב")
        .unwrap();

    assert_eq!(run.direction(), TextDirection::LeftToRight);

    // logical clusters:
    // A 0
    //   1
    // א 2
    // ב 4
    //
    // visual:
    // A <space> ב א
    assert_eq!(run.glyphs()[0].cluster(), 0);
    assert_eq!(run.glyphs()[1].cluster(), 1);
    assert_eq!(run.glyphs()[2].cluster(), 4);
    assert_eq!(run.glyphs()[3].cluster(), 2);
}

#[test]
fn rtl_mirroring_covers_unicode_mirrored_pairs() {
    static CHARACTERS: [char; 4] = ['ب', '≤', '≥', '?'];

    let font = TestFont {
        characters: &CHARACTERS,
        advance: px(5),
        kerning: px(0),
    };

    let mut registry = FontRegistry::<1>::default();
    let font_id = registry.register(&font).unwrap();
    let run = SimpleShaper::new()
        .shape_line(&registry, font_id, 16, "ب≤ب")
        .unwrap();

    assert_eq!(run.direction(), TextDirection::RightToLeft);
    assert_eq!(run.glyphs()[1].glyph(), font.glyph_id('≥').unwrap());
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

    let run = SimpleShaper::new()
        .shape_line(&registry, latin_id, 16, "ب A")
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

    let run = SimpleShaper::new()
        .shape_line(&registry, font_id, 16, "ب (AB)")
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
fn bracket_pair_after_arabic_stays_with_it_at_line_end() {
    static CHARACTERS: [char; 6] = ['a', ' ', 'ب', '(', ')', '?'];

    let font = TestFont {
        characters: &CHARACTERS,
        advance: px(5),
        kerning: px(0),
    };

    let mut registry = FontRegistry::<1>::default();
    let font_id = registry.register(&font).unwrap();
    let run = SimpleShaper::new()
        .shape_line(&registry, font_id, 16, "a ب (ب)")
        .unwrap();

    assert_eq!(run.direction(), TextDirection::LeftToRight);

    // logical UTF-8 clusters:
    //     a 0, space 1, ب 2, space 4, ( 5, ب 6, ) 8
    //
    // visual:
    //     a <space> ) ب ( <space> ب
    //
    // the brackets enclose only Arabic and follow Arabic, so both join the RTL run
    // and are mirrored
    let clusters: [usize; 7] = core::array::from_fn(|index| run.glyphs()[index].cluster());
    assert_eq!(clusters, [0, 1, 8, 6, 5, 4, 2]);

    assert_eq!(run.glyphs()[2].glyph(), font.glyph_id('(').unwrap());
    assert_eq!(run.glyphs()[4].glyph(), font.glyph_id(')').unwrap());
}

#[test]
fn bracket_pair_after_arabic_stays_with_it_before_english() {
    static CHARACTERS: [char; 7] = ['a', 'b', ' ', 'ب', '(', ')', '?'];

    let font = TestFont {
        characters: &CHARACTERS,
        advance: px(5),
        kerning: px(0),
    };

    let mut registry = FontRegistry::<1>::default();
    let font_id = registry.register(&font).unwrap();
    let run = SimpleShaper::new()
        .shape_line(&registry, font_id, 16, "a ب (ب) b")
        .unwrap();

    assert_eq!(run.direction(), TextDirection::LeftToRight);

    // logical UTF-8 clusters:
    //     a 0, space 1, ب 2, space 4, ( 5, ب 6, ) 8, space 9, b 10
    //
    // visual:
    //     a <space> ) ب ( <space> ب <space> b
    let clusters: [usize; 9] = core::array::from_fn(|index| run.glyphs()[index].cluster());
    assert_eq!(clusters, [0, 1, 8, 6, 5, 4, 2, 9, 10]);

    assert_eq!(run.glyphs()[2].glyph(), font.glyph_id('(').unwrap());
    assert_eq!(run.glyphs()[4].glyph(), font.glyph_id(')').unwrap());
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

    let run = SimpleShaper::new()
        .shape_line(&registry, font_id, 16, "AB (ب)")
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
    let run = SimpleShaper::new()
        .shape_line(&registry, primary_id, 16, "ب (AB)")
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

/// a variable font whose advances follow the weight: 4 px at normal, 7 px at bold
struct WeightedFont {
    characters: &'static [char],
}

impl FontFace for WeightedFont {
    fn weight_range(&self) -> FontWeightRange {
        FontWeightRange::new(FontWeight::NORMAL, FontWeight::NORMAL, FontWeight::BOLD)
    }

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

    fn glyph_metrics(&self, _: GlyphId, _: u16) -> Option<GlyphMetrics> {
        Some(GlyphMetrics::new(1, 1, px(0), px(-1), px(4)))
    }

    fn glyph_advance_with_properties(
        &self,
        properties: FontProperties,
        _: GlyphId,
        _: u16,
    ) -> Option<Pixels> {
        Some(px(i32::from(properties.weight().value() / 100)))
    }

    fn rasterize(&self, _: GlyphId, _: u16, coverage: &mut [u8]) -> Result<(), FontRasterError> {
        let Some(pixel) = coverage.first_mut() else {
            return Err(FontRasterError::BufferTooSmall);
        };

        *pixel = 255;

        Ok(())
    }
}

#[test]
fn mirrored_brackets_keep_the_runs_weight() {
    static CHARACTERS: [char; 4] = ['ب', '(', ')', '?'];

    let font = WeightedFont {
        characters: &CHARACTERS,
    };

    let mut registry = FontRegistry::<1>::default();
    let font_id = registry.register(&font).unwrap();
    let run = SimpleShaper::with_properties(FontProperties::new(FontWeight::BOLD))
        .shape_line(&registry, font_id, 16, "(ب)")
        .unwrap();

    assert_eq!(run.direction(), TextDirection::RightToLeft);

    // visual: the logical ')' first, drawn as '(', then ب, then '(' drawn as ')'
    assert_eq!(run.glyphs()[0].glyph(), font.glyph_id('(').unwrap());
    assert_eq!(run.glyphs()[2].glyph(), font.glyph_id(')').unwrap());

    for glyph in run.glyphs() {
        assert_eq!(glyph.font_instance().weight(), FontWeight::BOLD);
        assert_eq!(glyph.advance(), px(7));
    }
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

    let measured = shaper.measure_line(&registry, font_id, 16, "بب").unwrap();

    // logical glyph order:
    //     initial -> final
    // visual glyph order:
    //     final -> initial
    // the visual pair has kerning -3, so:
    //     5 + (5 - 3) = 7
    assert_eq!(measured.advance(), px(7));

    let run = shaper.shape_line(&registry, font_id, 16, "بب").unwrap();

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
    let run = SimpleShaper::new()
        .shape_line(&registry, font_id, 16, "AA")
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

    let run = SimpleShaper::new()
        .shape_line(
            &registry,
            arabic_id,
            16,
            concat!(
                "\u{0628}", // beh
                "\u{064E}", // fatha
                "\u{0651}", // shadda
                "\u{0650}", // kasra
            ),
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
fn latin_combining_marks_stack_on_their_base() {
    static CHARACTERS: [char; 5] = ['e', 'b', '\u{0301}', '\u{0323}', '?'];

    let font = TestFont {
        characters: &CHARACTERS,
        advance: px(5),
        kerning: px(0),
    };

    let mut registry = FontRegistry::<1>::default();
    let font_id = registry.register(&font).unwrap();
    let shaper = SimpleShaper::new();

    // e + combining acute + combining dot below, then b
    let text = "e\u{0301}\u{0323}b";
    let run = shaper.shape_line(&registry, font_id, 16, text).unwrap();

    assert_eq!(run.len(), 4);

    for glyph in &run.glyphs()[..3] {
        assert_eq!(glyph.cluster(), 0);
    }

    assert_eq!(run.glyphs()[1].mark_placement(), Some(MarkPlacement::Above));
    assert_eq!(run.glyphs()[2].mark_placement(), Some(MarkPlacement::Below));
    assert_eq!(run.glyphs()[1].offset(), Offset::new(px(-5), px(-2)));
    assert_eq!(run.glyphs()[2].offset(), Offset::new(px(-5), px(2)));
    assert_eq!(run.advance(), px(10));

    // e is 0..1, the marks 1..5, so the first legal break comes after both marks
    assert_eq!(
        shaper.next_cluster_boundary(&registry, font_id, 16, text, 0),
        Some(5),
    );
}

#[test]
fn hebrew_points_are_placed_by_combining_class() {
    static CHARACTERS: [char; 4] = ['ב', '\u{05B0}', '\u{05B9}', '?'];

    let font = TestFont {
        characters: &CHARACTERS,
        advance: px(5),
        kerning: px(0),
    };

    let mut registry = FontRegistry::<1>::default();
    let font_id = registry.register(&font).unwrap();
    // bet + sheva + holam
    let run = SimpleShaper::new()
        .shape_line(&registry, font_id, 16, "ב\u{05B0}\u{05B9}")
        .unwrap();

    assert_eq!(run.len(), 3);
    assert_eq!(run.glyphs()[1].mark_placement(), Some(MarkPlacement::Below));
    assert_eq!(run.glyphs()[2].mark_placement(), Some(MarkPlacement::Above));
    assert_eq!(run.advance(), px(5));
}

#[test]
fn hebrew_dagesh_is_centered_in_its_letter() {
    static GLYPHS: [(char, u16, u16, i32, i32); 3] = [
        ('ב', 6, 8, 0, -8),
        ('\u{05BC}', 2, 2, 0, -2),
        ('?', 1, 1, 0, -1),
    ];

    let font = BoxFont {
        glyphs: &GLYPHS,
        advance: px(7),
    };

    let mut registry = FontRegistry::<1>::default();
    let font_id = registry.register(&font).unwrap();
    // bet + dagesh
    let run = SimpleShaper::new()
        .shape_line(&registry, font_id, 16, "ב\u{05BC}")
        .unwrap();

    assert_eq!(run.len(), 2);
    assert_eq!(
        run.glyphs()[1].mark_placement(),
        Some(MarkPlacement::Center)
    );
    // back over the 7px advance, then (6 - 2) / 2 = 2 in. The base's ink spans -8..0,
    // so the 2px dagesh spans -5..-3, which is 3 above its own bearing of -2
    assert_eq!(run.glyphs()[1].offset(), Offset::new(px(-5), px(-3)));
    assert_eq!(run.glyphs()[1].advance(), px(0));
    assert_eq!(run.advance(), px(7));
}

#[test]
fn spacing_marks_join_the_cluster_and_keep_their_advance() {
    static CHARACTERS: [char; 3] = ['क', '\u{093F}', '?'];

    let font = TestFont {
        characters: &CHARACTERS,
        advance: px(5),
        kerning: px(0),
    };

    let mut registry = FontRegistry::<1>::default();
    let font_id = registry.register(&font).unwrap();
    // ka + vowel sign i
    let run = SimpleShaper::new()
        .shape_line(&registry, font_id, 16, "क\u{093F}")
        .unwrap();

    assert_eq!(run.len(), 2);
    assert_eq!(run.glyphs()[1].cluster(), 0);
    assert_eq!(run.glyphs()[1].mark_placement(), None);
    assert_eq!(run.advance(), px(10));
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
    let run = SimpleShaper::new()
        .shape_line(
            &registry,
            font_id,
            16,
            concat!(
                "\u{0628}", // beh
                "\u{064E}", // fatha
                "\u{0651}", // shadda
            ),
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
    let run = SimpleShaper::new()
        .shape_line(
            &registry,
            font_id,
            16,
            concat!(
                "\u{0644}", // lam
                "\u{0627}", // alef
                "\u{064E}", // fatha
            ),
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

#[test]
fn rtl_cursive_attachment_chains_bases_and_marks() {
    static ARABIC: [char; 5] = [
        '\u{FE90}', // beh final
        '\u{FE92}', // beh medial
        '\u{064E}', // fatha
        '\u{FE91}', // beh initial
        '?',
    ];

    let arabic = CursiveTestFont {
        characters: &ARABIC,
        advance: px(7),
    };

    let mut registry = FontRegistry::<1>::default();
    let font_id = registry.register(&arabic).unwrap();
    let run = SimpleShaper::new()
        .shape_line(
            &registry,
            font_id,
            16,
            concat!(
                "\u{0628}", // beh
                "\u{0628}", // beh
                "\u{064E}", // fatha
                "\u{0628}", // beh
            ),
        )
        .unwrap();

    assert_eq!(run.direction(), TextDirection::RightToLeft);
    assert_eq!(run.len(), 4);

    let final_beh = run.glyphs()[0];
    let medial_beh = run.glyphs()[1];
    let fatha = run.glyphs()[2];
    let initial_beh = run.glyphs()[3];

    assert_eq!(final_beh.glyph(), arabic.glyph_id('\u{FE90}').unwrap());
    assert_eq!(medial_beh.glyph(), arabic.glyph_id('\u{FE92}').unwrap());
    assert_eq!(fatha.glyph(), arabic.glyph_id('\u{064E}').unwrap());
    assert_eq!(initial_beh.glyph(), arabic.glyph_id('\u{FE91}').unwrap());

    // first base remains the visual chain root.
    assert_eq!(final_beh.offset(), Offset::ZERO);
    assert_eq!(final_beh.advance(), px(7));
    // desired origin delta from final -> medial is (4, -2).
    // nominal advance is 7, therefore:
    //     x adjustment = 4 - 7 = -3
    assert_eq!(medial_beh.offset(), Offset::new(px(-3), px(-2)));
    assert_eq!(medial_beh.advance(), px(4));
    // the mark anchor is (1, -3) relative to its cursively shifted medial base.
    // horizontal mark offset:
    //     1 - 7 = -6
    // vertical mark offset:
    //     -2 + -3 = -5
    assert_eq!(fatha.offset(), Offset::new(px(-6), px(-5)));
    assert_eq!(fatha.advance(), px(0));

    // medial -> initial origin delta is (3, 1).
    // x: 3 - 7 = -4
    // y: -2 + 1 = -1
    assert_eq!(initial_beh.offset(), Offset::new(px(-4), px(-1)));
    assert_eq!(initial_beh.advance(), px(3));
    // 7 + 4 + 0 + 3
    assert_eq!(run.advance(), px(14));
}

#[test]
fn gsub_shapes_arabic_contextual_forms_without_presentation_glyphs() {
    static ARABIC: [char; 2] = [
        '\u{0628}', // beh base character only
        '?',
    ];

    let arabic = GsubArabicFont {
        characters: &ARABIC,
        advance: px(6),
    };

    let mut registry = FontRegistry::<1>::default();
    let font_id = registry.register(&arabic).unwrap();
    let run = SimpleShaper::new()
        .shape_line(&registry, font_id, 16, "\u{0628}\u{0628}\u{0628}")
        .unwrap();

    assert_eq!(run.direction(), TextDirection::RightToLeft);
    assert_eq!(run.len(), 3);

    // logical Arabic forms are:
    //     initial, medial, final
    // after RTL visual ordering:
    //     final, medial, initial
    assert_eq!(run.glyphs()[0].glyph(), GlyphId::new(103));
    assert_eq!(run.glyphs()[1].glyph(), GlyphId::new(102));
    assert_eq!(run.glyphs()[2].glyph(), GlyphId::new(101));
    assert_eq!(run.glyphs()[0].cluster(), 4);
    assert_eq!(run.glyphs()[1].cluster(), 2);
    assert_eq!(run.glyphs()[2].cluster(), 0);
    assert_eq!(run.advance(), px(18));
}

#[test]
fn gsub_shapes_contextual_forms_for_letters_without_presentation_forms() {
    static ARABIC: [char; 2] = [
        '\u{08A0}', // beh with small v below, which has no Presentation Forms
        '?',
    ];

    let arabic = GsubArabicFont {
        characters: &ARABIC,
        advance: px(6),
    };

    let mut registry = FontRegistry::<1>::default();
    let font_id = registry.register(&arabic).unwrap();
    let run = SimpleShaper::new()
        .shape_line(&registry, font_id, 16, "\u{08A0}\u{08A0}\u{08A0}")
        .unwrap();

    assert_eq!(run.direction(), TextDirection::RightToLeft);
    assert_eq!(run.len(), 3);

    // visual RTL order: final, medial, initial
    assert_eq!(run.glyphs()[0].glyph(), GlyphId::new(103));
    assert_eq!(run.glyphs()[1].glyph(), GlyphId::new(102));
    assert_eq!(run.glyphs()[2].glyph(), GlyphId::new(101));
    assert_eq!(run.advance(), px(18));
}

#[test]
fn gsub_required_ligature_shapes_lam_alef_without_presentation_glyphs() {
    static ARABIC: [char; 3] = [
        '\u{0644}', // lam base
        '\u{0627}', // alef base
        '?',
    ];

    let arabic = GsubArabicFont {
        characters: &ARABIC,
        advance: px(9),
    };

    let mut registry = FontRegistry::<1>::default();
    let font_id = registry.register(&arabic).unwrap();
    let run = SimpleShaper::new()
        .shape_line(&registry, font_id, 16, "\u{0644}\u{0627}")
        .unwrap();

    assert_eq!(run.direction(), TextDirection::RightToLeft);
    assert_eq!(run.len(), 1);
    assert_eq!(run.glyphs()[0].glyph(), GlyphId::new(300));
    assert_eq!(run.glyphs()[0].cluster(), 0);
    assert_eq!(run.glyphs()[0].final_ligature_component(), Some(1));
    assert_eq!(run.advance(), px(9));
}

#[test]
fn prepared_shaper_matches_regular_shaper_across_repeated_calls() {
    static CHARACTERS: [char; 4] = ['A', 'B', ' ', '\u{0628}'];

    static FONT: TestFont = TestFont {
        characters: &CHARACTERS,
        advance: px(8),
        kerning: px(-1),
    };

    let mut registry = FontRegistry::<1>::default();

    let font = registry.register(&FONT).unwrap();

    let shaper = SimpleShaper::new();
    let prepared = shaper.prepare(&registry, font);

    for text in ["AB", "A B", "A \u{0628} B"] {
        let mut reused = std::vec::Vec::new();
        let reused_summary = prepared
            .shape_line_with(&registry, 16, text, "", |glyph| reused.push(glyph))
            .unwrap();

        let regular = shaper.shape_line(&registry, font, 16, text).unwrap();

        assert_eq!(reused, regular.glyphs);
        assert_eq!(reused_summary.advance(), regular.advance());

        for from in text.char_indices().map(|(offset, _)| offset) {
            assert_eq!(
                prepared.next_cluster_boundary(&registry, 16, text, from),
                shaper.next_cluster_boundary(&registry, font, 16, text, from),
            );
        }
    }
}

/// the glyphs of `text` and `suffix` from the streaming shaper
fn shape_streamed_line<const CHUNK: usize, const FONTS: usize>(
    registry: &FontRegistry<'_, FONTS>,
    font_id: FontId,
    text: &str,
    suffix: &str,
) -> Result<(std::vec::Vec<ShapedGlyph>, Pixels), ShapeError> {
    let mut glyphs = std::vec::Vec::new();
    let summary = SimpleShaper::new()
        .prepare(registry, font_id)
        .shape_line_in_chunks::<CHUNK, FONTS, 1>(registry, 16, text, suffix, None, |glyph| {
            glyphs.push(glyph)
        })?;

    assert_eq!(summary.glyph_count(), glyphs.len());

    Ok((glyphs, summary.advance()))
}

/// the most glyphs any cluster of `glyphs` holds
fn longest_cluster(glyphs: &[ShapedGlyph]) -> usize {
    glyphs
        .chunk_by(|first, second| first.cluster() == second.cluster())
        .map(<[ShapedGlyph]>::len)
        .max()
        .unwrap_or(0)
}

/// more glyphs than any test line has, so each of its runs shapes in one chunk
const UNCHUNKED: usize = 1024;

fn assert_streamed_line_matches<const CHUNK: usize>(
    registry: &FontRegistry<'_, 1>,
    font_id: FontId,
    text: &str,
    suffix: &str,
) {
    let whole = shape_streamed_line::<UNCHUNKED, 1>(registry, font_id, text, suffix);
    let streamed = shape_streamed_line::<CHUNK, 1>(registry, font_id, text, suffix);

    match (&whole, &streamed) {
        // a cluster must fit in a chunk
        (Ok((glyphs, _)), Err(ShapeError::BufferTooSmall)) => assert!(
            longest_cluster(glyphs) > CHUNK,
            "{text:?} + {suffix:?} in chunks of {CHUNK}"
        ),
        _ => assert_eq!(
            whole, streamed,
            "{text:?} + {suffix:?} in chunks of {CHUNK}"
        ),
    }
}

/// lines from the shaping tests above, and random lines mixing their scripts, shape to
/// the same glyphs whole and streamed in small chunks, with and without an ellipsis
#[test]
fn streamed_lines_match_whole_lines() {
    static TEXT: [char; 12] = ['a', 'b', 'A', ' ', '1', ',', '"', '(', ')', '≤', '≥', '?'];
    static BIDI: [char; 9] = ['ب', 'א', 'ב', ' ', 'A', '(', ')', '1', '?'];
    static MARKS: [char; 6] = ['e', '\u{0301}', '\u{0323}', 'ב', '\u{05B0}', '?'];
    static ANCHORED: [char; 4] = ['\u{FE8F}', '\u{064E}', '\u{0651}', '?'];
    static LAM_ALEF: [char; 3] = ['\u{FEFB}', '\u{064E}', '?'];
    static CURSIVE: [char; 5] = ['\u{FE90}', '\u{FE92}', '\u{064E}', '\u{FE91}', '?'];
    static GSUB: [char; 3] = ['\u{0644}', '\u{0627}', '?'];

    let text_font = TestFont {
        characters: &TEXT,
        advance: px(5),
        kerning: px(1),
    };
    let bidi_font = TestFont {
        characters: &BIDI,
        advance: px(5),
        kerning: px(-1),
    };
    let marks_font = TestFont {
        characters: &MARKS,
        advance: px(5),
        kerning: px(0),
    };
    let anchored_font = AnchoredMarkFont {
        characters: &ANCHORED,
        advance: px(5),
    };
    let lam_alef_font = LigatureAnchorFont {
        characters: &LAM_ALEF,
        advance: px(8),
    };
    let cursive_font = CursiveTestFont {
        characters: &CURSIVE,
        advance: px(7),
    };
    let gsub_font = GsubArabicFont {
        characters: &GSUB,
        advance: px(5),
    };
    let fonts: [&dyn FontFace; 7] = [
        &text_font,
        &bidi_font,
        &marks_font,
        &anchored_font,
        &lam_alef_font,
        &cursive_font,
        &gsub_font,
    ];

    let lines = [
        "",
        "ب 12",
        "a 1,\"",
        "ب ب ب ب",
        "ب AB",
        "ب AB ب",
        "אב A",
        "A אב A",
        "ب≤≥",
        "ب (AB) ب",
        "a (ب) b",
        "ab ب (a) ()",
        "AB (ب)",
        "ب()",
        "e\u{0301}\u{0323}b",
        "ב\u{05B0}\u{05B9}",
        "\u{0628}\u{064E}\u{0651}",
        "\u{0644}\u{0627}\u{064E}",
        "\u{0628}\u{0628}\u{064E}\u{0628}",
        "\u{0644}\u{0627}",
    ];

    let mut alphabet: std::vec::Vec<char> = lines.iter().flat_map(|line| line.chars()).collect();
    alphabet.extend([
        '\u{200D}', '\u{200C}', '\u{00AD}', '[', ']', '«', '»', '١', '.', '-',
    ]);
    alphabet.sort_unstable();
    alphabet.dedup();

    let mut seed = 0x2545_f491_4f6c_dd1d_u64;
    let mut random = move |bound: usize| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed % bound as u64) as usize
    };
    let random_lines: std::vec::Vec<std::string::String> = (0..80)
        .map(|_| {
            let len = random(140);
            (0..len).map(|_| alphabet[random(alphabet.len())]).collect()
        })
        .collect();

    for font in fonts {
        let mut registry = FontRegistry::<1>::default();
        let font_id = registry.register(font).unwrap();

        let all_lines = lines
            .iter()
            .copied()
            .chain(random_lines.iter().map(std::string::String::as_str));

        for line in all_lines {
            for suffix in ["", crate::text_layout::ELLIPSIS] {
                assert_streamed_line_matches::<3>(&registry, font_id, line, suffix);
                assert_streamed_line_matches::<4>(&registry, font_id, line, suffix);
                assert_streamed_line_matches::<7>(&registry, font_id, line, suffix);
                assert_streamed_line_matches::<64>(&registry, font_id, line, suffix);
            }
        }
    }
}

#[test]
fn ellipsis_after_a_one_letter_rtl_line_stays_together() {
    static CHARACTERS: [char; 3] = ['ب', '.', '?'];

    let font = TestFont {
        characters: &CHARACTERS,
        advance: px(5),
        kerning: px(0),
    };
    let mut registry = FontRegistry::<1>::default();
    let font_id = registry.register(&font).unwrap();

    // the letter's cluster and the first dot's are both 0, which must not make them one
    // cluster when the line is reversed
    let expected = [(2, 2), (2, 1), (2, 0), (1, 0)];

    let (glyphs, _) =
        shape_streamed_line::<64, 1>(&registry, font_id, "ب", crate::text_layout::ELLIPSIS)
            .unwrap();
    let order: std::vec::Vec<_> = glyphs
        .iter()
        .map(|glyph| (glyph.glyph().value(), glyph.cluster()))
        .collect();

    assert_eq!(order, expected);
}
