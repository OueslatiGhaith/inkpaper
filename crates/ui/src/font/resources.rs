#[cfg(feature = "alloc")]
use alloc::boxed::Box;

#[cfg(feature = "metrics")]
use crate::GlyphCacheMetrics;
use crate::{FontFamilyId, FontInstance, FontStyle, FontWeight, ResolvedFont};

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
    pub const fn registry(&self) -> &FontRegistry<'font, FONTS> {
        &self.registry
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

    #[cfg(feature = "alloc")]
    pub fn register_owned_face(
        &mut self,
        family: FontFamilyId,
        font: Box<dyn FontFace>,
    ) -> Result<FontId, FontRegistryError> {
        self.registry.register_owned_face(family, font)
    }

    #[cfg(feature = "alloc")]
    pub fn clear_owned_faces(&mut self) {
        self.registry.clear_owned_faces();
    }

    pub fn resolve(&self, id: FontId) -> Option<(FontId, &dyn FontFace)> {
        self.registry.resolve_with_id(id)
    }

    pub fn resolve_weight(&self, weight: FontWeight) -> Option<ResolvedFont<'_>> {
        self.registry.resolve_weight(weight)
    }

    pub fn resolve_family_weight(
        &self,
        family: FontFamilyId,
        weight: FontWeight,
    ) -> Option<ResolvedFont<'_>> {
        self.registry.resolve_family_weight(family, weight)
    }

    pub fn resolve_family_font(
        &self,
        family: FontFamilyId,
        weight: FontWeight,
        style: FontStyle,
    ) -> Option<ResolvedFont<'_>> {
        self.registry.resolve_family_font(family, weight, style)
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

    /// The registry with the glyph cache, so glyphs resolved from the
    /// registry can be rasterized while they're borrowed.
    pub fn registry_and_glyph_cache(
        &mut self,
    ) -> (
        &FontRegistry<'font, FONTS>,
        &mut GlyphCache<GLYPH_SLOTS, GLYPH_BYTES>,
    ) {
        (&self.registry, &mut self.cache)
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

    pub fn resolve_glyph(&self, preferred: FontId, character: char) -> Option<ResolvedGlyph<'_>> {
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
