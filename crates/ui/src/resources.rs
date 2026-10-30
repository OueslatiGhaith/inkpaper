#[cfg(feature = "alloc")]
use alloc::boxed::Box;

#[cfg(feature = "metrics")]
use crate::GlyphCacheMetrics;
use crate::{
    AvailableSpace, FontFace, FontFamilyId, FontId, FontInstance, FontRegistry, FontRegistryError,
    FontResources, FontStyle, FontWeight, GlyphBitmap, GlyphCache, GlyphCacheError, GlyphId,
    ImageId, ImagePaint, ImageRegistry, ImageRegistryError, ImageResource, ImageSource, PaintError,
    PaintResources, Painter, Rect, ResolvedFont, ResolvedTextStyle, Size, TextMeasurer,
    pair_cache::{PAIR_POSITIONING_CACHE_SLOTS, PairPositioningCache},
    rendering::{
        image_paint::paint_image,
        text_paint::{paint_text, paint_text_run},
    },
};

/// how much text painting did since the last reset
#[cfg(feature = "metrics")]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct TextPaintMetrics {
    pub draw_calls: u64,
    pub shaped_glyphs: u64,
    pub pair_cache_lookups: u64,
    pub pair_cache_hits: u64,
    pub pair_cache_misses: u64,
    pub pair_cache_collisions: u64,
}

pub struct RuntimeResources<
    'resource,
    const FONTS: usize,
    const GLYPH_SLOTS: usize,
    const GLYPH_BYTES: usize,
    const IMAGES: usize,
> {
    fonts: FontResources<'resource, FONTS, GLYPH_SLOTS, GLYPH_BYTES>,
    images: ImageRegistry<'resource, IMAGES>,
    pair_positioning_cache: PairPositioningCache<PAIR_POSITIONING_CACHE_SLOTS>,
    #[cfg(feature = "metrics")]
    text_paint_metrics: TextPaintMetrics,
}

impl<const FONTS: usize, const GLYPH_SLOTS: usize, const GLYPH_BYTES: usize, const IMAGES: usize>
    Default for RuntimeResources<'_, FONTS, GLYPH_SLOTS, GLYPH_BYTES, IMAGES>
{
    fn default() -> Self {
        Self {
            fonts: FontResources::default(),
            images: ImageRegistry::default(),
            pair_positioning_cache: PairPositioningCache::default(),
            #[cfg(feature = "metrics")]
            text_paint_metrics: TextPaintMetrics::default(),
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

    #[cfg(feature = "metrics")]
    pub fn reset_text_paint_metrics(&mut self) {
        self.text_paint_metrics = TextPaintMetrics::default();
        self.pair_positioning_cache.reset_metrics();
    }

    #[cfg(feature = "metrics")]
    pub const fn text_paint_metrics(&self) -> TextPaintMetrics {
        let pair_cache = self.pair_positioning_cache.metrics();

        TextPaintMetrics {
            pair_cache_lookups: pair_cache.lookups,
            pair_cache_hits: pair_cache.hits,
            pair_cache_misses: pair_cache.misses,
            pair_cache_collisions: pair_cache.collisions,
            ..self.text_paint_metrics
        }
    }

    #[cfg(feature = "metrics")]
    fn record_text_draw(&mut self, shaped_glyphs: u64) {
        let metrics = &mut self.text_paint_metrics;

        metrics.draw_calls = metrics.draw_calls.saturating_add(1);
        metrics.shaped_glyphs = metrics.shaped_glyphs.saturating_add(shaped_glyphs);
    }
}

impl<const FONTS: usize, const GLYPH_SLOTS: usize, const GLYPH_BYTES: usize, const IMAGES: usize>
    PaintResources for RuntimeResources<'_, FONTS, GLYPH_SLOTS, GLYPH_BYTES, IMAGES>
{
    fn paint_text<P: Painter>(
        &mut self,
        painter: &mut P,
        text: &str,
        bounds: Rect,
        style: ResolvedTextStyle,
        clip: Option<Rect>,
    ) -> Result<(), PaintError<P::Error>> {
        if bounds.width().is_non_positive() || bounds.height().is_non_positive() {
            return Ok(());
        }

        let Some(clip) = (match clip {
            Some(clip) => clip.intersection(bounds),
            None => Some(bounds),
        }) else {
            return Ok(());
        };

        let (registry, glyph_cache) = self.fonts.registry_and_glyph_cache();

        let font = registry
            .resolve_family_font(style.font_family, style.font_weight, style.font_style)
            .expect("painting text requires a default font");

        let shaped_glyphs = paint_text(
            painter,
            text,
            bounds,
            clip,
            registry,
            glyph_cache,
            font,
            style,
        )?;

        #[cfg(feature = "metrics")]
        self.record_text_draw(shaped_glyphs);

        #[cfg(not(feature = "metrics"))]
        let _ = shaped_glyphs;

        Ok(())
    }

    fn paint_text_run<P: Painter>(
        &mut self,
        painter: &mut P,
        text: &str,
        bounds: Rect,
        style: ResolvedTextStyle,
        clip: Option<Rect>,
    ) -> Result<(), PaintError<P::Error>> {
        if text.is_empty() || bounds.width().is_non_positive() || bounds.height().is_non_positive()
        {
            return Ok(());
        }

        // glyph ink may overflow a run, so only the caller's clip applies
        let clip = clip.unwrap_or(bounds);

        let (registry, glyph_cache) = self.fonts.registry_and_glyph_cache();

        let font = registry
            .resolve_family_font(style.font_family, style.font_weight, style.font_style)
            .expect("painting text requires a default font");

        let shaped_glyphs = paint_text_run(
            painter,
            text,
            bounds,
            clip,
            registry,
            glyph_cache,
            font,
            style,
            &mut self.pair_positioning_cache,
        )?;

        #[cfg(feature = "metrics")]
        self.record_text_draw(shaped_glyphs);

        #[cfg(not(feature = "metrics"))]
        let _ = shaped_glyphs;

        Ok(())
    }

    fn paint_image<P: Painter>(
        &mut self,
        painter: &mut P,
        source: ImageSource,
        bounds: Rect,
        paint: ImagePaint,
        clip: Option<Rect>,
    ) -> Result<(), PaintError<P::Error>> {
        if bounds.width().is_non_positive() || bounds.height().is_non_positive() {
            return Ok(());
        }

        let Some(image) = self.images.get(source.id()) else {
            debug_assert!(false, "image {:?} is not registered", source.id());
            return Ok(());
        };

        debug_assert_eq!(
            image.size(),
            source.size(),
            "registered image size differs from ImageSource size for {:?}",
            source.id(),
        );

        let Some(clip) = (match clip {
            Some(clip) => clip.intersection(bounds),
            None => Some(bounds),
        }) else {
            return Ok(());
        };

        paint_image(painter, image, bounds, paint, Some(clip)).map_err(PaintError::Painter)
    }
}

impl<const FONTS: usize, const GLYPH_SLOTS: usize, const GLYPH_BYTES: usize, const IMAGES: usize>
    TextMeasurer for RuntimeResources<'_, FONTS, GLYPH_SLOTS, GLYPH_BYTES, IMAGES>
{
    fn measure_text(
        &self,
        text: &str,
        style: ResolvedTextStyle,
        available_width: AvailableSpace,
    ) -> Size {
        self.fonts.measure_text(text, style, available_width)
    }
}
