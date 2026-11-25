use skrifa::{
    GlyphId16,
    raw::{FontData as RawFontData, FontRead, tables::gsub::MultipleSubstFormat1},
};

use super::gsub::apply_multiple_single_output;

const SINGLE_OUTPUT_MULTIPLE_SUBSTITUTION: &[u8] = &[
    // MultipleSubst format 1
    0x00, 0x01, // coverage offset = 8
    0x00, 0x08, // sequence count = 1
    0x00, 0x01, // sequence[0] offset = 14
    0x00, 0x0E, // Coverage format 1
    0x00, 0x01, // glyph count = 1
    0x00, 0x01, // covered glyph = 5
    0x00, 0x05, // Sequence:
    // substitute count = 1
    0x00, 0x01, // substitute glyph = 9
    0x00, 0x09,
];

const MULTI_OUTPUT_MULTIPLE_SUBSTITUTION: &[u8] = &[
    // MultipleSubst format 1
    0x00, 0x01, // coverage offset = 8
    0x00, 0x08, // sequence count = 1
    0x00, 0x01, // sequence[0] offset = 14
    0x00, 0x0E, // Coverage format 1
    0x00, 0x01, // glyph count = 1
    0x00, 0x01, // covered glyph = 5
    0x00, 0x05, // Sequence:
    // substitute count = 2
    0x00, 0x02, // substitute glyphs = 9, 10
    0x00, 0x09, 0x00, 0x0A,
];

#[test]
fn single_output_multiple_substitution_is_accepted() {
    let substitution =
        MultipleSubstFormat1::read(RawFontData::new(SINGLE_OUTPUT_MULTIPLE_SUBSTITUTION)).unwrap();

    let result = apply_multiple_single_output(&substitution, GlyphId16::new(5));

    assert_eq!(result, Some(GlyphId16::new(9)));
}

#[test]
fn multi_output_multiple_substitution_is_rejected() {
    let substitution =
        MultipleSubstFormat1::read(RawFontData::new(MULTI_OUTPUT_MULTIPLE_SUBSTITUTION)).unwrap();

    let result = apply_multiple_single_output(&substitution, GlyphId16::new(5));

    assert_eq!(result, None);
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
