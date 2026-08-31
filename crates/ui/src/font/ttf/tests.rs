    use ttf_parser::opentype_layout::LookupSubtable;

    use super::*;

    #[test]
    fn non_zero_winding_fills_between_intersections() {
        let mut coverage = [0u8; 6];

        let intersections = [
            Intersection {
                x: 1.0,
                winding: -1,
            },
            Intersection { x: 5.0, winding: 1 },
        ];

        for _ in 0..SUPERSAMPLE_Y {
            accumulate_scanline(6, 0, &mut coverage, &intersections);
        }

        normalize_coverage(&mut coverage);

        assert_eq!(coverage, [0, 255, 255, 255, 255, 0,],);
    }

    #[test]
    fn scanline_builder_tracks_winding_direction() {
        let mut builder = ScanlineBuilder::new(1.0, 0, 0, 2.5);

        builder.add_segment(
            RasterPoint { x: 5.0, y: 1.0 },
            RasterPoint { x: 5.0, y: 5.0 },
        );

        builder.add_segment(
            RasterPoint { x: 1.0, y: 5.0 },
            RasterPoint { x: 1.0, y: 1.0 },
        );

        builder.sort_intersections();

        assert_eq!(builder.intersections().len(), 2,);
        assert_eq!(builder.intersections()[0].x, 1.0,);
        assert_eq!(builder.intersections()[0].winding, -1,);
        assert_eq!(builder.intersections()[1].x, 5.0,);
        assert_eq!(builder.intersections()[1].winding, 1,);
    }

    #[test]
    fn curve_subdivision_is_bounded() {
        let steps = curve_steps(&[
            RasterPoint::ZERO,
            RasterPoint {
                x: 10_000.0,
                y: 10_000.0,
            },
            RasterPoint {
                x: 20_000.0,
                y: 0.0,
            },
        ]);

        assert_eq!(steps, MAX_CURVE_STEPS,);
    }

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
        let substitution = <SubstitutionSubtable<'static> as LookupSubtable<'static>>::parse(
            SINGLE_OUTPUT_MULTIPLE_SUBSTITUTION,
            2,
        )
        .unwrap();

        let result = apply_single_output_substitution(substitution, TtfGlyphId(5));

        assert_eq!(result, Some(TtfGlyphId(9)),);
    }

    #[test]
    fn multi_output_multiple_substitution_is_rejected() {
        let substitution = <SubstitutionSubtable<'static> as LookupSubtable<'static>>::parse(
            MULTI_OUTPUT_MULTIPLE_SUBSTITUTION,
            2,
        )
        .unwrap();

        let result = apply_single_output_substitution(substitution, TtfGlyphId(5));

        assert_eq!(result, None);
    }
