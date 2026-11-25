mod shaping {
    use alloc::vec::Vec;

    use crate::{
        FontData, FontFace, FontProperties, FontRegistry, FontWeight, ShapedGlyph, SimpleShaper,
        TtfFont, px,
    };

    const INTER: TtfFont<'static> = TtfFont::from_data(
        FontData::new(include_bytes!(
            "../../../../app/assets/fonts/InterVariable.ttf"
        )),
        0,
    );

    const LIBRON: TtfFont<'static> = TtfFont::from_data(
        FontData::new(include_bytes!(
            "../../../../app/assets/fonts/Libron-Regular.ttf"
        )),
        0,
    );

    fn shape(
        registry: &FontRegistry<'_, 4>,
        font: crate::FontId,
        properties: FontProperties,
        size_px: u16,
        text: &str,
    ) -> (Vec<ShapedGlyph>, crate::Pixels) {
        let mut glyphs = Vec::new();
        let shaper = SimpleShaper::with_properties(properties).prepare(registry, font);
        let summary = shaper
            .shape_line_with(registry, size_px, text, "", |glyph| glyphs.push(glyph))
            .unwrap();
        (glyphs, summary.advance())
    }

    #[test]
    fn harfrust_applies_kerning_pairs() {
        let mut registry = FontRegistry::<4>::default();
        let font = registry.register(&LIBRON).unwrap();

        let (glyphs_av, width_av) = shape(&registry, font, FontProperties::NORMAL, 32, "AV");
        let (_, width_a) = shape(&registry, font, FontProperties::NORMAL, 32, "A");
        let (_, width_v) = shape(&registry, font, FontProperties::NORMAL, 32, "V");

        assert_eq!(glyphs_av.len(), 2);
        assert!(
            width_av < width_a + width_v,
            "expected kerning on \"AV\": shaped={width_av:?}, sum={:?}",
            width_a + width_v
        );
    }

    #[test]
    fn harfrust_forms_standard_ligatures() {
        let mut registry = FontRegistry::<4>::default();
        let font = registry.register(&LIBRON).unwrap();

        let (glyphs, width) = shape(&registry, font, FontProperties::NORMAL, 24, "fi");

        assert_eq!(glyphs.len(), 1, "expected \"fi\" ligature in Libron");
        assert_eq!(glyphs[0].cluster(), 0);
        assert!(width > px(0));
    }

    #[test]
    fn harfrust_applies_variable_weight_axis() {
        let mut registry = FontRegistry::<4>::default();
        let font = registry.register(&INTER).unwrap();

        let (_, light_width) = shape(
            &registry,
            font,
            FontProperties::new(FontWeight::LIGHT),
            24,
            "Typography",
        );
        let (_, bold_width) = shape(
            &registry,
            font,
            FontProperties::new(FontWeight::BOLD),
            24,
            "Typography",
        );

        assert!(
            bold_width > light_width,
            "expected Inter bold ({bold_width:?}) wider than light ({light_width:?})"
        );
    }

    #[test]
    fn harfrust_strips_default_ignorables() {
        let mut registry = FontRegistry::<4>::default();
        let font = registry.register(&INTER).unwrap();

        let (plain_glyphs, plain_width) =
            shape(&registry, font, FontProperties::NORMAL, 20, "ebook");
        let (ignorable_glyphs, ignorable_width) = shape(
            &registry,
            font,
            FontProperties::NORMAL,
            20,
            "e\u{00AD}\u{200B}book",
        );

        assert_eq!(ignorable_glyphs.len(), plain_glyphs.len());
        assert_eq!(ignorable_width, plain_width);
    }

    #[test]
    fn harfrust_falls_back_to_second_face_and_preserves_clusters() {
        let mut registry = FontRegistry::<4>::default();
        let libron = registry.register(&LIBRON).unwrap();
        let inter = registry.register(&INTER).unwrap();

        // Pick a character present in Inter but absent from Libron if available,
        // otherwise verify fallback to replacement glyph works without panicking.
        let sample = if LIBRON.glyph_id('€').is_none() && INTER.glyph_id('€').is_some() {
            "A€B"
        } else {
            "A☃B"
        };

        let (glyphs, width) = shape(&registry, libron, FontProperties::NORMAL, 20, sample);
        assert_eq!(glyphs.len(), 3);
        assert_eq!(glyphs[0].cluster(), 0);
        assert_eq!(glyphs[1].cluster(), 1);
        assert_eq!(glyphs[2].cluster(), sample.len() - 1);
        if sample == "A€B" {
            assert_eq!(glyphs[0].font(), libron);
            assert_eq!(glyphs[1].font(), inter);
            assert_eq!(glyphs[2].font(), libron);
        }
        assert!(width > px(0));
    }

    #[cfg(feature = "hinting")]
    #[test]
    fn hinted_faces_share_harfrust_shaping_results() {
        use alloc::sync::Arc;

        use crate::{HintedOwnedTtfFont, HintedTtfFont};

        let hinted = HintedTtfFont::new(LIBRON);
        let owned = HintedOwnedTtfFont::parse(Arc::<[u8]>::from(LIBRON.data().bytes()), 0).unwrap();

        let mut reg_hinted = FontRegistry::<4>::default();
        let id_hinted = reg_hinted.register(&hinted).unwrap();
        let mut reg_owned = FontRegistry::<4>::default();
        let id_owned = reg_owned.register(&owned).unwrap();

        let (g1, w1) = shape(&reg_hinted, id_hinted, FontProperties::NORMAL, 24, "AV fi");
        let (g2, w2) = shape(&reg_owned, id_owned, FontProperties::NORMAL, 24, "AV fi");

        assert_eq!(g1, g2);
        assert_eq!(w1, w2);
    }
}

#[cfg(feature = "hinting")]
mod hinting {
    use alloc::{vec, vec::Vec};

    use crate::{FontData, FontFace, FontProperties, FontWeight, HintedTtfFont, TtfFont};

    const INTER: TtfFont<'static> = TtfFont::from_data(
        FontData::new(include_bytes!(
            "../../../../app/assets/fonts/InterVariable.ttf"
        )),
        0,
    );

    const SIZES: [u16; 4] = [14, 20, 26, 32];

    const TEXT: &str =
        "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789.,;:!?\u{201c}";

    const NORMAL: FontProperties = FontProperties::new(FontWeight::NORMAL);

    fn rasterize(font: &dyn FontFace, character: char, size: u16) -> (usize, Vec<u8>) {
        let glyph = font.glyph_id(character).unwrap();
        let metrics = font.glyph_metrics(NORMAL, glyph, size).unwrap();
        let mut coverage = vec![0; metrics.coverage_bytes().unwrap()];

        font.rasterize(NORMAL, glyph, size, &mut coverage).unwrap();

        (usize::from(metrics.width), coverage)
    }

    /// the glyph cache sizes each bitmap from the metrics before rasterizing into it
    #[test]
    fn hinted_glyphs_rasterize_into_the_bitmap_their_metrics_describe() {
        let font = HintedTtfFont::new(INTER);

        for size in SIZES {
            for character in TEXT.chars() {
                let (_, coverage) = rasterize(&font, character, size);

                assert!(
                    coverage.iter().any(|value| *value > 0),
                    "{character:?} at {size}px"
                );
            }
        }
    }

    #[test]
    fn hinting_keeps_unhinted_advances() {
        let font = HintedTtfFont::new(INTER);

        for size in SIZES {
            for character in TEXT.chars() {
                let glyph = INTER.glyph_id(character).unwrap();

                assert_eq!(
                    font.glyph_metrics(NORMAL, glyph, size).unwrap().advance,
                    INTER.glyph_metrics(NORMAL, glyph, size).unwrap().advance,
                );
            }
        }
    }

    #[test]
    fn hinted_stems_share_one_width_at_each_size() {
        let font = HintedTtfFont::new(INTER);

        for size in SIZES {
            let x_height = -font
                .glyph_metrics(NORMAL, font.glyph_id('x').unwrap(), size)
                .unwrap()
                .bearing_y
                .get();

            let widths: Vec<_> = "lihnmur"
                .chars()
                .flat_map(|character| {
                    let glyph = font.glyph_id(character).unwrap();
                    let top = font
                        .glyph_metrics(NORMAL, glyph, size)
                        .unwrap()
                        .bearing_y
                        .get();
                    let (width, coverage) = rasterize(&font, character, size);

                    // the middle of the x-height crosses only the stems of these letters
                    let row = usize::try_from(-x_height / 2 - top).unwrap();

                    // the device inks coverage of at least 7/16
                    let inked: Vec<_> = coverage[row * width..][..width]
                        .iter()
                        .map(|value| *value >= 112)
                        .collect();

                    inked
                        .chunk_by(|a, b| a == b)
                        .filter(|run| run[0])
                        .map(<[bool]>::len)
                        .collect::<Vec<_>>()
                })
                .collect();

            assert!(
                widths.iter().all(|width| *width == widths[0]),
                "stem widths at {size}px: {widths:?}"
            );
        }
    }
}

mod owned {
    use alloc::vec::Vec;

    use crate::{
        FontData, FontFace, FontProperties, FontStyle, OwnedTtfFont, TtfFont, TtfFontError,
    };

    const ITALIC: &[u8] = include_bytes!("../../../../app/assets/fonts/Libron-Italic.ttf");

    #[test]
    fn owned_font_reads_like_the_borrowed_one() {
        let borrowed = TtfFont::from_data(FontData::new(ITALIC), 0);
        let owned = OwnedTtfFont::parse(Vec::from(ITALIC), 0).unwrap();

        let glyph = borrowed.glyph_id('g').unwrap();

        assert_eq!(owned.glyph_id('g'), Some(glyph));
        assert_eq!(owned.style(), FontStyle::Italic);
        assert_eq!(owned.weight_range(), borrowed.weight_range());
        assert_eq!(
            owned.metrics(FontProperties::NORMAL, 20),
            borrowed.metrics(FontProperties::NORMAL, 20)
        );
        assert_eq!(
            owned.glyph_metrics(FontProperties::NORMAL, glyph, 20),
            borrowed.glyph_metrics(FontProperties::NORMAL, glyph, 20)
        );
    }

    #[test]
    fn owned_font_rejects_bytes_that_are_not_a_font() {
        assert_eq!(
            OwnedTtfFont::parse(Vec::from(&b"not a font"[..]), 0).err(),
            Some(TtfFontError::InvalidFont)
        );
    }

    #[cfg(feature = "hinting")]
    #[test]
    fn hinted_owned_font_matches_hinted_borrowed_font() {
        use alloc::vec;

        use crate::{HintedOwnedTtfFont, HintedTtfFont};

        let borrowed = HintedTtfFont::new(TtfFont::from_data(FontData::new(ITALIC), 0));
        let owned = HintedOwnedTtfFont::parse(Vec::from(ITALIC), 0).unwrap();

        let glyph = borrowed.glyph_id('g').unwrap();
        let borrowed_metrics = borrowed
            .glyph_metrics(FontProperties::NORMAL, glyph, 20)
            .unwrap();
        let owned_metrics = owned
            .glyph_metrics(FontProperties::NORMAL, glyph, 20)
            .unwrap();

        assert_eq!(owned_metrics, borrowed_metrics);

        let bytes = owned_metrics.coverage_bytes().unwrap();
        let mut borrowed_cov = vec![0u8; bytes];
        let mut owned_cov = vec![0u8; bytes];

        borrowed
            .rasterize(FontProperties::NORMAL, glyph, 20, &mut borrowed_cov)
            .unwrap();
        owned
            .rasterize(FontProperties::NORMAL, glyph, 20, &mut owned_cov)
            .unwrap();

        assert_eq!(owned_cov, borrowed_cov);
    }
}
