    use super::*;
    use crate::px;

    struct TestFont {
        fill: u8,
    }

    impl FontFace for TestFont {
        fn glyph_id(&self, character: char) -> Option<GlyphId> {
            let value = u32::from(character);

            u16::try_from(value).ok().map(GlyphId::new)
        }

        fn metrics(&self, size_px: u16) -> FontMetrics {
            FontMetrics::new(px(i32::from(size_px)), px(0), px(0))
        }

        fn glyph_metrics(&self, _glyph: GlyphId, _size_px: u16) -> Option<GlyphMetrics> {
            Some(GlyphMetrics::new(2, 2, px(0), px(-2), px(3)))
        }

        fn rasterize(
            &self,
            _glyph: GlyphId,
            _size_px: u16,
            coverage: &mut [u8],
        ) -> Result<(), FontRasterError> {
            if coverage.len() < 4 {
                return Err(FontRasterError::BufferTooSmall);
            }

            coverage[..4].fill(self.fill);

            Ok(())
        }
    }

    #[test]
    fn registry_assigns_stable_font_ids() {
        let first = TestFont { fill: 10 };
        let second = TestFont { fill: 20 };

        let mut registry = FontRegistry::<2>::default();

        let first_id = registry.register(&first).unwrap();
        let second_id = registry.register(&second).unwrap();

        assert_eq!(first_id, FontId::DEFAULT);
        assert_eq!(second_id, FontId::new(1));
        assert_eq!(registry.len(), 2);
    }

    #[test]
    fn registry_reports_capacity_exhaustion() {
        let first = TestFont { fill: 10 };
        let second = TestFont { fill: 20 };

        let mut registry = FontRegistry::<1>::default();
        registry.register(&first).unwrap();

        assert_eq!(registry.register(&second), Err(FontRegistryError::Full));
    }

    #[test]
    fn glyph_cache_returns_cached_coverage() {
        let mut registry = FontRegistry::<1>::default();

        let font = TestFont { fill: 173 };
        let font_id = registry.register(&font).unwrap();
        let glyph = font.glyph_id('A').unwrap();

        let mut storage = [0u8; 16];
        let mut cache = GlyphCache::<4>::new(&mut storage);

        {
            let bitmap = cache
                .get_or_rasterize(&registry, font_id, glyph, 16)
                .unwrap();

            assert_eq!(bitmap.coverage(), &[173, 173, 173, 173]);
        }

        assert_eq!(cache.used_bytes(), 4);

        {
            let bitmap = cache
                .get_or_rasterize(&registry, font_id, glyph, 16)
                .unwrap();

            assert_eq!(bitmap.coverage(), &[173, 173, 173, 173]);
        }

        assert_eq!(cache.used_bytes(), 4);
    }

    #[test]
    fn glyph_cache_flushes_when_storage_is_full() {
        let mut registry = FontRegistry::<1>::default();

        let font = TestFont { fill: 99 };
        let font_id = registry.register(&font).unwrap();
        let mut storage = [0u8; 6];
        let mut cache = GlyphCache::<4>::new(&mut storage);

        let first = font.glyph_id('A').unwrap();
        let second = font.glyph_id('B').unwrap();

        cache
            .get_or_rasterize(&registry, font_id, first, 16)
            .unwrap();

        assert_eq!(cache.used_bytes(), 4);

        cache
            .get_or_rasterize(&registry, font_id, second, 16)
            .unwrap();

        // the second 4-byte glyph cannot fit after the first,so the cache resets
        // and begins a fresh generation.
        assert_eq!(cache.used_bytes(), 4);
    }

    #[test]
    fn glyph_larger_than_entire_cache_is_rejected() {
        let mut registry = FontRegistry::<1>::default();

        let font = TestFont { fill: 255 };
        let font_id = registry.register(&font).unwrap();
        let glyph = font.glyph_id('A').unwrap();

        let mut storage = [0u8; 3];
        let mut cache = GlyphCache::<4>::new(&mut storage);

        let result = cache.get_or_rasterize(&registry, font_id, glyph, 16);

        assert!(matches!(result, Err(GlyphCacheError::GlyphTooLarge)));
    }
