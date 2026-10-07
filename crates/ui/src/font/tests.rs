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

    let mut cache = GlyphCache::<4, 16>::default();

    {
        let bitmap = cache
            .get_or_rasterize(&registry, font_id, glyph, 16)
            .unwrap();

        assert_eq!(bitmap.coverage(), &[173, 173, 173, 173]);
    }

    assert_eq!(cache.used_bytes(), 4);
    assert_eq!(cache.capacity_bytes(), 16);

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

    let mut cache = GlyphCache::<4, 6>::default();

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

    let mut cache = GlyphCache::<4, 3>::default();

    let result = cache.get_or_rasterize(&registry, font_id, glyph, 16);

    assert!(matches!(result, Err(GlyphCacheError::GlyphTooLarge)));
}

#[test]
fn font_weight_clamps_to_opentype_range() {
    assert_eq!(FontWeight::new(0), FontWeight::new(1));
    assert_eq!(FontWeight::new(1001), FontWeight::new(1000));
    assert_eq!(FontWeight::new(650).value(), 650);
}

#[test]
fn registry_resolves_closest_weight_in_family() {
    let regular = TestFont { fill: 10 };
    let bold = TestFont { fill: 20 };

    let mut registry = FontRegistry::<2>::default();
    let family = registry.register_family().unwrap();

    let regular_id = registry
        .register_face_with_weight(family, FontWeight::NORMAL, &regular)
        .unwrap();

    let bold_id = registry
        .register_face_with_weight(family, FontWeight::BOLD, &bold)
        .unwrap();

    let resolved_regular = registry
        .resolve_family_weight(family, FontWeight::MEDIUM)
        .unwrap();

    let resolved_bold = registry
        .resolve_family_weight(family, FontWeight::SEMIBOLD)
        .unwrap();

    assert_eq!(resolved_regular.id(), regular_id);
    assert_eq!(resolved_regular.weight(), FontWeight::NORMAL);

    assert_eq!(resolved_bold.id(), bold_id);
    assert_eq!(resolved_bold.weight(), FontWeight::BOLD);
}

struct ItalicTestFont(TestFont);

impl FontFace for ItalicTestFont {
    fn style(&self) -> FontStyle {
        FontStyle::Italic
    }

    fn glyph_id(&self, character: char) -> Option<GlyphId> {
        self.0.glyph_id(character)
    }

    fn metrics(&self, size_px: u16) -> FontMetrics {
        self.0.metrics(size_px)
    }

    fn glyph_metrics(&self, glyph: GlyphId, size_px: u16) -> Option<GlyphMetrics> {
        self.0.glyph_metrics(glyph, size_px)
    }

    fn rasterize(
        &self,
        glyph: GlyphId,
        size_px: u16,
        coverage: &mut [u8],
    ) -> Result<(), FontRasterError> {
        self.0.rasterize(glyph, size_px, coverage)
    }
}

#[test]
fn registry_prefers_the_requested_style_over_weight() {
    let regular = TestFont { fill: 10 };
    let bold = TestFont { fill: 20 };
    let italic = ItalicTestFont(TestFont { fill: 30 });

    let mut registry = FontRegistry::<3>::default();
    let family = registry.register_family().unwrap();

    let regular_id = registry
        .register_face_with_weight(family, FontWeight::NORMAL, &regular)
        .unwrap();
    let bold_id = registry
        .register_face_with_weight(family, FontWeight::BOLD, &bold)
        .unwrap();
    let italic_id = registry
        .register_face_with_weight(family, FontWeight::NORMAL, &italic)
        .unwrap();

    let resolve = |weight, style| {
        registry
            .resolve_family_font(family, weight, style)
            .unwrap()
            .id()
    };

    assert_eq!(resolve(FontWeight::NORMAL, FontStyle::Normal), regular_id);
    assert_eq!(resolve(FontWeight::BOLD, FontStyle::Normal), bold_id);
    assert_eq!(resolve(FontWeight::NORMAL, FontStyle::Italic), italic_id);
    assert_eq!(resolve(FontWeight::BOLD, FontStyle::Italic), italic_id);
}

#[test]
fn registry_falls_back_to_upright_faces_without_italics() {
    let regular = TestFont { fill: 10 };

    let mut registry = FontRegistry::<1>::default();
    let family = registry.register_family().unwrap();

    let regular_id = registry.register_face(family, &regular).unwrap();

    let resolved = registry
        .resolve_family_font(family, FontWeight::NORMAL, FontStyle::Italic)
        .unwrap();

    assert_eq!(resolved.id(), regular_id);
}

#[test]
fn registry_rejects_unregistered_family() {
    let font = TestFont { fill: 10 };
    let mut registry = FontRegistry::<1>::default();

    assert_eq!(
        registry.register_face_with_weight(FontFamilyId::new(7), FontWeight::NORMAL, &font),
        Err(FontRegistryError::InvalidFamily),
    );
}

struct VariableTestFont;

impl FontFace for VariableTestFont {
    fn weight_range(&self) -> FontWeightRange {
        FontWeightRange::new(
            FontWeight::LIGHT,
            FontWeight::NORMAL,
            FontWeight::EXTRA_BOLD,
        )
    }

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

        coverage[..4].fill(100);

        Ok(())
    }

    fn rasterize_with_properties(
        &self,
        properties: FontProperties,
        _glyph: GlyphId,
        _size_px: u16,
        coverage: &mut [u8],
    ) -> Result<(), FontRasterError> {
        if coverage.len() < 4 {
            return Err(FontRasterError::BufferTooSmall);
        }

        let value = u8::try_from(properties.weight().value() / 4).unwrap_or(u8::MAX);

        coverage[..4].fill(value);

        Ok(())
    }
}

#[test]
fn registry_preserves_requested_weight_inside_variable_range() {
    let font = VariableTestFont;

    let mut registry = FontRegistry::<1>::default();

    let family = registry.register_family().unwrap();

    registry.register_face(family, &font).unwrap();

    let resolved = registry
        .resolve_family_weight(family, FontWeight::new(650))
        .unwrap();

    assert_eq!(resolved.weight(), FontWeight::new(650));
}

#[test]
fn glyph_cache_distinguishes_variable_weights() {
    let font = VariableTestFont;

    let mut registry = FontRegistry::<1>::default();

    let family = registry.register_family().unwrap();

    registry.register_face(family, &font).unwrap();

    let weight_650 = registry
        .resolve_family_weight(family, FontWeight::new(650))
        .unwrap()
        .instance();

    let weight_700 = registry
        .resolve_family_weight(family, FontWeight::BOLD)
        .unwrap()
        .instance();

    let glyph = font.glyph_id('A').unwrap();

    let mut cache = GlyphCache::<4, 16>::default();

    {
        let bitmap = cache
            .get_or_rasterize(&registry, weight_650, glyph, 16)
            .unwrap();

        assert_eq!(bitmap.coverage(), &[162, 162, 162, 162]);
    }

    {
        let bitmap = cache
            .get_or_rasterize(&registry, weight_700, glyph, 16)
            .unwrap();

        assert_eq!(bitmap.coverage(), &[175, 175, 175, 175]);
    }

    assert_eq!(cache.used_bytes(), 8);
}

#[cfg(feature = "metrics")]
#[test]
fn glyph_cache_metrics_track_linear_probe_collisions() {
    let mut registry = FontRegistry::<1>::default();

    let font = TestFont { fill: 173 };
    let font_id = registry.register(&font).unwrap();

    let first = font.glyph_id('A').unwrap();
    let colliding = font.glyph_id('E').unwrap();

    let mut cache = GlyphCache::<4, 16>::default();

    cache.reset_metrics();

    cache
        .get_or_rasterize(&registry, font_id, first, 16)
        .unwrap();

    cache
        .get_or_rasterize(&registry, font_id, colliding, 16)
        .unwrap();

    cache
        .get_or_rasterize(&registry, font_id, first, 16)
        .unwrap();

    cache
        .get_or_rasterize(&registry, font_id, colliding, 16)
        .unwrap();

    let metrics = cache.metrics();

    assert_eq!(metrics.lookups, 4);
    assert_eq!(metrics.hits, 2);
    assert_eq!(metrics.misses, 2);
    assert_eq!(metrics.collisions, 1);
    assert_eq!(metrics.rasterizations, 2);
    assert_eq!(metrics.clears, 0);
    assert_eq!(metrics.bytes_peak, 8);

    assert_eq!(cache.used_bytes(), 8);
}

#[cfg(feature = "metrics")]
#[test]
fn glyph_cache_metrics_track_capacity_clears() {
    let mut registry = FontRegistry::<1>::default();

    let font = TestFont { fill: 173 };
    let font_id = registry.register(&font).unwrap();

    let first = font.glyph_id('A').unwrap();
    let second = font.glyph_id('B').unwrap();
    let third = font.glyph_id('C').unwrap();

    // each glyph consumes four bytes. Ten bytes hold two glyphs, but the third miss
    // must begin a fresh cache generation.
    let mut cache = GlyphCache::<4, 10>::default();

    cache.reset_metrics();

    cache
        .get_or_rasterize(&registry, font_id, first, 16)
        .unwrap();

    cache
        .get_or_rasterize(&registry, font_id, first, 16)
        .unwrap();

    cache
        .get_or_rasterize(&registry, font_id, second, 16)
        .unwrap();

    assert_eq!(cache.used_bytes(), 8);

    cache
        .get_or_rasterize(&registry, font_id, third, 16)
        .unwrap();

    let metrics = cache.metrics();

    assert_eq!(metrics.lookups, 4);
    assert_eq!(metrics.hits, 1);
    assert_eq!(metrics.misses, 3);
    assert_eq!(metrics.collisions, 0);
    assert_eq!(metrics.rasterizations, 3);
    assert_eq!(metrics.clears, 1);
    assert_eq!(metrics.bytes_peak, 8);

    assert_eq!(cache.used_bytes(), 4);
}

#[test]
fn glyph_cache_preserves_colliding_entries_with_linear_probing() {
    let mut registry = FontRegistry::<1>::default();

    let font = TestFont { fill: 173 };
    let font_id = registry.register(&font).unwrap();

    let first = font.glyph_id('A').unwrap();
    let colliding = font.glyph_id('E').unwrap();

    // with four slots:
    //   17 % 4 == 1
    // and A (65) and E (69) differ by four, so both keys have the same home slot.
    let mut cache = GlyphCache::<4, 16>::default();

    cache
        .get_or_rasterize(&registry, font_id, first, 16)
        .unwrap();

    cache
        .get_or_rasterize(&registry, font_id, colliding, 16)
        .unwrap();

    assert_eq!(cache.used_bytes(), 8);

    // both entries must still be reachable. A direct-mapped cache would rerasterize both
    // accesses and grow the byte arena to 16 bytes.
    cache
        .get_or_rasterize(&registry, font_id, first, 16)
        .unwrap();

    cache
        .get_or_rasterize(&registry, font_id, colliding, 16)
        .unwrap();

    assert_eq!(cache.used_bytes(), 8);
}

#[test]
fn prepared_font_delegates_for_non_ttf_face() {
    let font = TestFont { fill: 42 };

    let mut registry = FontRegistry::<1>::default();
    let font_id = registry.register(&font).unwrap();

    let prepared = registry
        .prepare_instance(FontInstance::normal(font_id))
        .unwrap();

    let (glyph, advance) = prepared.glyph_id_and_advance('A', 16).unwrap();

    assert_eq!(glyph, GlyphId::new(u16::from(b'A')));
    assert_eq!(advance, px(3));
    assert_eq!(
        prepared.pair_positioning(GlyphId::new(1), GlyphId::new(2), 16, false),
        PairPositioning::Kerning(px(0)),
    );
}

#[cfg(feature = "alloc")]
#[test]
fn owned_faces_resolve_in_their_family_until_cleared() {
    let ui = TestFont { fill: 10 };
    let mut registry = FontRegistry::<2>::default();

    let ui_family = registry.register_family().unwrap();
    let custom_family = registry.register_family().unwrap();

    let ui_id = registry.register_face(ui_family, &ui).unwrap();
    let custom_id = registry
        .register_owned_face(
            custom_family,
            alloc::boxed::Box::new(TestFont { fill: 200 }),
        )
        .unwrap();

    let resolved = registry
        .resolve_family_font(custom_family, FontWeight::NORMAL, FontStyle::Normal)
        .unwrap();

    assert_eq!(resolved.id(), custom_id);

    registry.clear_owned_faces();

    assert_eq!(registry.len(), 1);
    assert!(registry.get(custom_id).is_none());
    assert!(registry.get(ui_id).is_some());

    // the family falls back to the default family, and so do its glyphs
    let resolved = registry
        .resolve_family_font(custom_family, FontWeight::NORMAL, FontStyle::Normal)
        .unwrap();

    assert_eq!(resolved.id(), ui_id);
    assert_eq!(
        registry.resolve_glyph(custom_id, 'A').unwrap().font(),
        ui_id
    );
}

#[cfg(feature = "alloc")]
#[test]
fn clearing_owned_faces_frees_their_ids_for_the_next_faces() {
    let ui = TestFont { fill: 10 };
    let mut registry = FontRegistry::<3>::default();

    let family = registry.register_family().unwrap();
    registry.register_face(family, &ui).unwrap();

    let first = registry
        .register_owned_face(family, alloc::boxed::Box::new(TestFont { fill: 1 }))
        .unwrap();
    registry
        .register_owned_face(family, alloc::boxed::Box::new(TestFont { fill: 2 }))
        .unwrap();

    assert_eq!(
        registry.register_owned_face(family, alloc::boxed::Box::new(TestFont { fill: 3 })),
        Err(FontRegistryError::Full),
    );

    registry.clear_owned_faces();

    assert_eq!(
        registry.register_owned_face(family, alloc::boxed::Box::new(TestFont { fill: 4 })),
        Ok(first),
    );
    assert_eq!(registry.len(), 2);
}

#[cfg(feature = "alloc")]
#[test]
fn glyph_cache_forgets_glyphs_of_cleared_faces() {
    let ui = TestFont { fill: 10 };
    let mut registry = FontRegistry::<2>::default();

    let family = registry.register_family().unwrap();
    registry.register_face(family, &ui).unwrap();

    let old = registry
        .register_owned_face(family, alloc::boxed::Box::new(TestFont { fill: 200 }))
        .unwrap();

    let mut cache = GlyphCache::<4, 16>::default();
    let glyph = GlyphId::new(65);

    let bitmap = cache.get_or_rasterize(&registry, old, glyph, 16).unwrap();
    assert_eq!(bitmap.coverage(), &[200; 4]);

    registry.clear_owned_faces();

    let new = registry
        .register_owned_face(family, alloc::boxed::Box::new(TestFont { fill: 99 }))
        .unwrap();

    assert_eq!(new, old);

    let bitmap = cache.get_or_rasterize(&registry, new, glyph, 16).unwrap();
    assert_eq!(bitmap.coverage(), &[99; 4]);
}
