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
        let mut output = [ShapedGlyph::EMPTY; 4];
        let run = SimpleShaper::new()
            .shape_into(
                &registry,
                font_id,
                16,
                concat!(
                    "\u{0628}", // beh
                    "\u{0628}", // beh
                    "\u{064E}", // fatha
                    "\u{0628}", // beh
                ),
                &mut output,
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
        let mut output = [ShapedGlyph::EMPTY; 3];
        let run = SimpleShaper::new()
            .shape_into(
                &registry,
                font_id,
                16,
                "\u{0628}\u{0628}\u{0628}",
                &mut output,
            )
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
        let mut output = [ShapedGlyph::EMPTY; 2];
        let run = SimpleShaper::new()
            .shape_into(&registry, font_id, 16, "\u{0644}\u{0627}", &mut output)
            .unwrap();

        assert_eq!(run.direction(), TextDirection::RightToLeft);
        assert_eq!(run.len(), 1);
        assert_eq!(run.glyphs()[0].glyph(), GlyphId::new(300));
        assert_eq!(run.glyphs()[0].cluster(), 0);
        assert_eq!(run.glyphs()[0].final_ligature_component(), Some(1));
        assert_eq!(run.advance(), px(9));
    }
