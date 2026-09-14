use crate::{
    FontFace, FontFamilyId, FontId, FontInstance, FontRegistry, FontRegistryError, FontResources,
    FontWeight, GlyphBitmap, GlyphCacheError, GlyphId, ImageId, ImageRegistry, ImageRegistryError,
    ImageResource, ImageSource, ResolvedFont, ResolvedTextStyle, Size, TextMeasurer,
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

    pub fn register_image(
        &mut self,
        image: &'resource dyn ImageResource,
    ) -> Result<ImageSource, ImageRegistryError> {
        self.images.register(image)
    }

    pub fn font_registry(&self) -> FontRegistry<'resource, FONTS> {
        self.fonts.registry()
    }

    pub fn resolve_font(&self, id: FontId) -> Option<(FontId, &'resource dyn FontFace)> {
        self.fonts.resolve(id)
    }

    pub fn resolve_font_weight(&self, weight: FontWeight) -> Option<ResolvedFont<'resource>> {
        self.fonts.resolve_weight(weight)
    }

    pub fn resolve_font_family_weight(
        &self,
        family: FontFamilyId,
        weight: FontWeight,
    ) -> Option<ResolvedFont<'resource>> {
        self.fonts.resolve_family_weight(family, weight)
    }

    pub fn glyph_bitmap(
        &mut self,
        font: FontInstance,
        glyph: GlyphId,
        size_px: u16,
    ) -> Result<GlyphBitmap<'_>, GlyphCacheError> {
        self.fonts.glyph_bitmap(font, glyph, size_px)
    }

    pub fn image(&self, id: ImageId) -> Option<&'resource dyn ImageResource> {
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
}

impl<const FONTS: usize, const GLYPH_SLOTS: usize, const GLYPH_BYTES: usize, const IMAGES: usize>
    TextMeasurer for RuntimeResources<'_, FONTS, GLYPH_SLOTS, GLYPH_BYTES, IMAGES>
{
    fn measure_text(&self, text: &str, style: ResolvedTextStyle, max_size: Size) -> Size {
        self.fonts.measure_text(text, style, max_size)
    }
}
