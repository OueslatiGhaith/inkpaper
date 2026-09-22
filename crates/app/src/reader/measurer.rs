use inkpaper_epub::{ChapterImage, FontWeight as ReaderFontWeight, ImageDimensions};
use inkpaper_reader::{ImageMeasurer, TextMeasurer, TextStyle as ReaderTextStyle};
use inkpaper_ui::{
    FontFamilyId, FontRegistry, FontRegistryError, FontWeight as UiFontWeight,
    PreparedSimpleShaper, ResolvedFont, ShapeError, ShapedGlyph, SimpleShaper,
};

use crate::reader::{
    images::ChapterImageMetrics,
    measure_cache::{
        READER_MEASURE_CACHE_SLOTS, READER_MEASURE_CACHE_TEXT_BYTES, ReaderMeasureCache,
        ReaderMeasureCacheLookup,
    },
};

const READER_SHAPING_GLYPHS: usize = 128;

pub(super) struct ReaderMeasurer {
    fonts: FontRegistry<'static, 1>,

    normal_shaper: PreparedSimpleShaper<'static>,
    bold_shaper: PreparedSimpleShaper<'static>,

    glyphs: [ShapedGlyph; READER_SHAPING_GLYPHS],
    images: ChapterImageMetrics,

    measure_cache: ReaderMeasureCache<READER_MEASURE_CACHE_SLOTS, READER_MEASURE_CACHE_TEXT_BYTES>,
}

impl ReaderMeasurer {
    pub(super) fn new(images: ChapterImageMetrics) -> Result<Self, FontRegistryError> {
        let mut fonts = FontRegistry::default();

        let family = fonts.register_family()?;

        fonts.register_face(family, crate::typography::ui_font())?;

        let normal_font = fonts
            .resolve_family_weight(FontFamilyId::DEFAULT, UiFontWeight::NORMAL)
            .expect("reader measurer always registers the UI font");

        let bold_font = fonts
            .resolve_family_weight(FontFamilyId::DEFAULT, UiFontWeight::BOLD)
            .expect("reader measurer always registers the UI font");

        let normal_shaper = SimpleShaper::with_properties(normal_font.properties())
            .prepare(&fonts, normal_font.id());

        let bold_shaper =
            SimpleShaper::with_properties(bold_font.properties()).prepare(&fonts, bold_font.id());

        Ok(Self {
            fonts,
            normal_shaper,
            bold_shaper,
            glyphs: [ShapedGlyph::EMPTY; READER_SHAPING_GLYPHS],
            images,
            measure_cache: ReaderMeasureCache::default(),
        })
    }

    fn resolve_font(&self, style: ReaderTextStyle) -> ResolvedFont<'static> {
        let weight = match style.font_weight() {
            ReaderFontWeight::Normal => UiFontWeight::NORMAL,
            ReaderFontWeight::Bold => UiFontWeight::BOLD,
        };

        self.fonts
            .resolve_family_weight(FontFamilyId::DEFAULT, weight)
            .expect("reader measurer always registers the UI font")
    }
}

impl TextMeasurer for ReaderMeasurer {
    type Error = ShapeError;

    fn measure_text(&mut self, text: &str, style: ReaderTextStyle) -> Result<u32, Self::Error> {
        let insertion_slot = match self.measure_cache.lookup(text, style) {
            ReaderMeasureCacheLookup::Hit(width) => {
                return Ok(width);
            }

            ReaderMeasureCacheLookup::Miss { slot, .. } => Some(slot),

            ReaderMeasureCacheLookup::Bypass => None,
        };

        let shaper = match style.font_weight() {
            ReaderFontWeight::Normal => &self.normal_shaper,

            ReaderFontWeight::Bold => &self.bold_shaper,
        };

        let summary = shaper.measure(&self.fonts, style.font_size(), text, &mut self.glyphs)?;

        let width = u32::try_from(summary.advance().non_negative().get()).unwrap_or(u32::MAX);

        if let Some(slot) = insertion_slot {
            self.measure_cache.insert(slot, text, style, width);
        }

        Ok(width)
    }

    fn line_height(&mut self, style: ReaderTextStyle) -> Result<u32, Self::Error> {
        let font = self.resolve_font(style);

        Ok(u32::try_from(
            font.metrics(style.font_size())
                .line_height()
                .non_negative()
                .get(),
        )
        .unwrap_or(u32::MAX))
    }

    fn next_boundary(
        &mut self,
        text: &str,
        from: usize,
        style: ReaderTextStyle,
    ) -> Result<Option<usize>, Self::Error> {
        let shaper = match style.font_weight() {
            ReaderFontWeight::Normal => &self.normal_shaper,

            ReaderFontWeight::Bold => &self.bold_shaper,
        };

        Ok(shaper.next_cluster_boundary(&self.fonts, style.font_size(), text, from))
    }
}

impl ImageMeasurer for ReaderMeasurer {
    fn image_dimensions(&mut self, image: &ChapterImage) -> Option<ImageDimensions> {
        self.images.dimensions(image.path())
    }
}
