#[cfg(feature = "alloc")]
use alloc::boxed::Box;

#[cfg(feature = "metrics")]
use crate::GlyphCacheMetrics;
use crate::{
    FontFace, FontFamilyId, FontId, FontInstance, FontRegistry, FontRegistryError, FontResources,
    FontStyle, FontWeight, GlyphBitmap, GlyphCache, GlyphCacheError, GlyphId, ImageId,
    ImageRegistry, ImageRegistryError, ImageResource, ImageSource, ResolvedFont, ResolvedTextStyle,
    Size, TextMeasurer,
};

pub struct RuntimeResources<
    'resource,
    const FONTS: usize,
    const GLYPH_SLOTS: usize,
    const GLYPH_BYTES: usize,
    const IMAGES: usize,
> {
    fonts: FontResources<'resource, FONTS, GLYPH_SLOTS, GLYPH_BYTES>,
    images: ImageRegistry<'resource, IMAGES>,
}

impl<const FONTS: usize, const GLYPH_SLOTS: usize, const GLYPH_BYTES: usize, const IMAGES: usize>
    Default for RuntimeResources<'_, FONTS, GLYPH_SLOTS, GLYPH_BYTES, IMAGES>
{
    fn default() -> Self {
        Self {
            fonts: FontResources::default(),
            images: ImageRegistry::default(),
        }
    }
}

impl<
    'resource,
    const FONTS: usize,
    const GLYPH_SLOTS: usize,
    const GLYPH_BYTES: usize,
    const IMAGES: usize,
> RuntimeResources<'resource, FONTS, GLYPH_SLOTS, GLYPH_BYTES, IMAGES>
{
    pub fn register_font(
        &mut self,
        font: &'resource dyn FontFace,
    ) -> Result<FontId, FontRegistryError> {
        self.fonts.register(font)
    }

    pub fn register_font_family(&mut self) -> Result<FontFamilyId, FontRegistryError> {
        self.fonts.register_family()
    }

    pub fn register_font_face(
        &mut self,
        family: FontFamilyId,
        font: &'resource dyn FontFace,
    ) -> Result<FontId, FontRegistryError> {
        self.fonts.register_face(family, font)
    }

    #[cfg(feature = "alloc")]
    pub fn register_owned_font_face(
        &mut self,
        family: FontFamilyId,
        font: Box<dyn FontFace>,
    ) -> Result<FontId, FontRegistryError> {
        self.fonts.register_owned_face(family, font)
    }

    #[cfg(feature = "alloc")]
    pub fn clear_owned_font_faces(&mut self) {
        self.fonts.clear_owned_faces();
    }

    pub fn register_image(
        &mut self,
        image: &'resource dyn ImageResource,
    ) -> Result<ImageSource, ImageRegistryError> {
        self.images.register(image)
    }

    #[cfg(feature = "alloc")]
    pub fn register_owned_image(
        &mut self,
        image: Box<dyn ImageResource>,
    ) -> Result<ImageSource, ImageRegistryError> {
        self.images.register_owned(image)
    }

    #[cfg(feature = "alloc")]
    pub fn clear_owned_images(&mut self) {
        self.images.clear_owned();
    }

    pub fn fonts_and_glyph_cache(
        &mut self,
    ) -> (
        &FontRegistry<'resource, FONTS>,
        &mut GlyphCache<GLYPH_SLOTS, GLYPH_BYTES>,
    ) {
        self.fonts.registry_and_glyph_cache()
    }

    pub const fn font_registry(&self) -> &FontRegistry<'resource, FONTS> {
        self.fonts.registry()
    }

    pub fn resolve_font(&self, id: FontId) -> Option<(FontId, &dyn FontFace)> {
        self.fonts.resolve(id)
    }

    pub fn resolve_font_weight(&self, weight: FontWeight) -> Option<ResolvedFont<'_>> {
        self.fonts.resolve_weight(weight)
    }

    pub fn resolve_font_family_weight(
        &self,
        family: FontFamilyId,
        weight: FontWeight,
    ) -> Option<ResolvedFont<'_>> {
        self.fonts.resolve_family_weight(family, weight)
    }

    pub fn resolve_font_family_style(
        &self,
        family: FontFamilyId,
        weight: FontWeight,
        style: FontStyle,
    ) -> Option<ResolvedFont<'_>> {
        self.fonts.resolve_family_font(family, weight, style)
    }

    pub fn glyph_bitmap(
        &mut self,
        font: FontInstance,
        glyph: GlyphId,
        size_px: u16,
    ) -> Result<GlyphBitmap<'_>, GlyphCacheError> {
        self.fonts.glyph_bitmap(font, glyph, size_px)
    }

    pub fn image(&self, id: ImageId) -> Option<&dyn ImageResource> {
        self.images.get(id)
    }

    pub fn clear_glyph_cache(&mut self) {
        self.fonts.clear_glyph_cache();
    }

    pub const fn glyph_cache_capacity_bytes(&self) -> usize {
        self.fonts.glyph_cache_capacity_bytes()
    }

    pub const fn glyph_cache_used_bytes(&self) -> usize {
        self.fonts.glyph_cache_used_bytes()
    }

    #[cfg(feature = "metrics")]
    pub fn reset_glyph_cache_metrics(&mut self) {
        self.fonts.reset_glyph_cache_metrics();
    }

    #[cfg(feature = "metrics")]
    pub const fn glyph_cache_metrics(&self) -> GlyphCacheMetrics {
        self.fonts.glyph_cache_metrics()
    }
}

impl<const FONTS: usize, const GLYPH_SLOTS: usize, const GLYPH_BYTES: usize, const IMAGES: usize>
    TextMeasurer for RuntimeResources<'_, FONTS, GLYPH_SLOTS, GLYPH_BYTES, IMAGES>
{
    fn measure_text(&self, text: &str, style: ResolvedTextStyle, max_size: Size) -> Size {
        self.fonts.measure_text(text, style, max_size)
    }
}
