#[cfg(feature = "metrics")]
use crate::GlyphCacheMetrics;
use crate::{FontFamilyId, FontInstance, FontWeight, ResolvedFont};

use super::{
    FontFace, FontId, GlyphId,
    cache::{GlyphBitmap, GlyphCache, GlyphCacheError},
    registry::{FontRegistry, FontRegistryError, ResolvedGlyph},
};

pub struct FontResources<
    'font,
    const FONTS: usize,
    const GLYPH_SLOTS: usize,
    const GLYPH_BYTES: usize,
> {
    registry: FontRegistry<'font, FONTS>,
    cache: GlyphCache<GLYPH_SLOTS, GLYPH_BYTES>,
}

impl<'font, const FONTS: usize, const GLYPH_SLOTS: usize, const GLYPH_BYTES: usize> Default
    for FontResources<'font, FONTS, GLYPH_SLOTS, GLYPH_BYTES>
{
    fn default() -> Self {
        Self {
            registry: FontRegistry::default(),
            cache: GlyphCache::default(),
        }
    }
}

impl<'font, const FONTS: usize, const GLYPH_SLOTS: usize, const GLYPH_BYTES: usize>
    FontResources<'font, FONTS, GLYPH_SLOTS, GLYPH_BYTES>
{
    pub fn registry(&self) -> FontRegistry<'font, FONTS> {
        self.registry
    }

    pub const fn len(&self) -> usize {
        self.registry.len()
    }

    pub const fn is_empty(&self) -> bool {
        self.registry.is_empty()
    }

    pub fn register(&mut self, font: &'font dyn FontFace) -> Result<FontId, FontRegistryError> {
        self.registry.register(font)
    }

    pub fn register_family(&mut self) -> Result<FontFamilyId, FontRegistryError> {
        self.registry.register_family()
    }

    pub fn register_face(
        &mut self,
        family: FontFamilyId,
        font: &'font dyn FontFace,
    ) -> Result<FontId, FontRegistryError> {
        self.registry.register_face(family, font)
    }

    pub fn resolve(&self, id: FontId) -> Option<(FontId, &'font dyn FontFace)> {
        self.registry.resolve_with_id(id)
    }

    pub fn resolve_weight(&self, weight: FontWeight) -> Option<ResolvedFont<'font>> {
        self.registry.resolve_weight(weight)
    }

    pub fn resolve_family_weight(
        &self,
        family: FontFamilyId,
        weight: FontWeight,
    ) -> Option<ResolvedFont<'font>> {
        self.registry.resolve_family_weight(family, weight)
    }

    pub fn glyph_bitmap(
        &mut self,
        font: FontInstance,
        glyph: GlyphId,
        size_px: u16,
    ) -> Result<GlyphBitmap<'_>, GlyphCacheError> {
        self.cache
            .get_or_rasterize(&self.registry, font, glyph, size_px)
    }

    pub fn clear_glyph_cache(&mut self) {
        self.cache.clear();
    }

    pub const fn glyph_cache_capacity_bytes(&self) -> usize {
        self.cache.capacity_bytes()
    }

    pub const fn glyph_cache_used_bytes(&self) -> usize {
        self.cache.used_bytes()
    }

    pub fn resolve_glyph(
        &self,
        preferred: FontId,
        character: char,
    ) -> Option<ResolvedGlyph<'font>> {
        self.registry.resolve_glyph(preferred, character)
    }

    #[cfg(feature = "metrics")]
    pub fn reset_glyph_cache_metrics(&mut self) {
        self.cache.reset_metrics();
    }

    #[cfg(feature = "metrics")]
    pub const fn glyph_cache_metrics(&self) -> GlyphCacheMetrics {
        self.cache.metrics()
    }
}
