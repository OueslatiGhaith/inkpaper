use inkpaper_epub::{
    ChapterImage, FontStyle as ReaderFontStyle, FontWeight as ReaderFontWeight, ImageDimensions,
};
use inkpaper_reader::{ImageMeasurer, TextMeasurer, TextStyle as ReaderTextStyle};
use inkpaper_ui::{
    FontFamilyId, FontRegistry, FontRegistryError, PreparedSimpleShaper, ResolvedFont, ShapeError,
    ShapedGlyph, SimpleShaper,
};

use crate::{
    fonts::CardFont,
    reader::{
        images::ChapterImageMetrics,
        measure_cache::{
            READER_MEASURE_CACHE_SLOTS, READER_MEASURE_CACHE_TEXT_BYTES, ReaderMeasureCache,
            ReaderMeasureCacheLookup,
        },
    },
    reader_page::reader_font,
    typography::{self, CARD_FAMILY, FONT_FACES, READER_FAMILY},
};

const READER_SHAPING_GLYPHS: usize = 128;

/// The faces the reader measures with, registered like the runtime's, and
/// the family it measures in.
pub(crate) struct ReaderFonts {
    registry: FontRegistry<'static, FONT_FACES>,
    family: FontFamilyId,
}

impl ReaderFonts {
    /// The built-in faces, and `card`'s when the reader uses a card font.
    pub(crate) fn new(card: Option<&CardFont>) -> Result<Self, FontRegistryError> {
        let mut registry = FontRegistry::default();

        typography::register_in(&mut registry)?;

        let family = match card {
            Some(card) => {
                typography::register_card_font_in(&mut registry, card)?;
                CARD_FAMILY
            }
            None => READER_FAMILY,
        };

        Ok(Self { registry, family })
    }
}

/// Measures with shapers prepared from `fonts`, which it borrows since what
/// a registry resolves borrows the registry.
pub(super) struct ReaderMeasurer<'fonts> {
    fonts: &'fonts ReaderFonts,

    /// indexed by [`shaper_index`]
    shapers: [PreparedSimpleShaper<'fonts>; 4],

    glyphs: [ShapedGlyph; READER_SHAPING_GLYPHS],
    images: ChapterImageMetrics,

    measure_cache: ReaderMeasureCache<READER_MEASURE_CACHE_SLOTS, READER_MEASURE_CACHE_TEXT_BYTES>,
}

impl<'fonts> ReaderMeasurer<'fonts> {
    pub(super) fn new(fonts: &'fonts ReaderFonts, images: ChapterImageMetrics) -> Self {
        let shapers = [
            (ReaderFontWeight::Normal, ReaderFontStyle::Normal),
            (ReaderFontWeight::Bold, ReaderFontStyle::Normal),
            (ReaderFontWeight::Normal, ReaderFontStyle::Italic),
            (ReaderFontWeight::Bold, ReaderFontStyle::Italic),
        ]
        .map(|(weight, style)| {
            let font = resolve_font(fonts, weight, style);

            SimpleShaper::with_properties(font.properties()).prepare(&fonts.registry, font.id())
        });

        Self {
            fonts,
            shapers,
            glyphs: [ShapedGlyph::EMPTY; READER_SHAPING_GLYPHS],
            images,
            measure_cache: ReaderMeasureCache::default(),
        }
    }
}

fn shaper_index(weight: ReaderFontWeight, style: ReaderFontStyle) -> usize {
    let bold = usize::from(weight == ReaderFontWeight::Bold);
    let italic = usize::from(style == ReaderFontStyle::Italic);

    bold + 2 * italic
}

fn resolve_font(
    fonts: &ReaderFonts,
    weight: ReaderFontWeight,
    style: ReaderFontStyle,
) -> ResolvedFont<'_> {
    let (weight, style) = reader_font(weight, style);

    fonts
        .registry
        .resolve_family_font(fonts.family, weight, style)
        .expect("reader measurer always registers the reader fonts")
}

impl TextMeasurer for ReaderMeasurer<'_> {
    type Error = ShapeError;

    fn measure_text(&mut self, text: &str, style: ReaderTextStyle) -> Result<u32, Self::Error> {
        let insertion_slot = match self.measure_cache.lookup(text, style) {
            ReaderMeasureCacheLookup::Hit(width) => {
                return Ok(width);
            }

            ReaderMeasureCacheLookup::Miss { slot, .. } => Some(slot),

            ReaderMeasureCacheLookup::Bypass => None,
        };

        let shaper = &self.shapers[shaper_index(style.font_weight(), style.font_style())];

        let summary = shaper.measure(
            &self.fonts.registry,
            style.font_size(),
            text,
            &mut self.glyphs,
        )?;

        let width = u32::try_from(summary.advance().non_negative().get()).unwrap_or(u32::MAX);

        if let Some(slot) = insertion_slot {
            self.measure_cache.insert(slot, text, style, width);
        }

        Ok(width)
    }

    fn line_height(&mut self, style: ReaderTextStyle) -> Result<u32, Self::Error> {
        let font = resolve_font(self.fonts, style.font_weight(), style.font_style());

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
        let shaper = &self.shapers[shaper_index(style.font_weight(), style.font_style())];

        Ok(shaper.next_cluster_boundary(&self.fonts.registry, style.font_size(), text, from))
    }
}

impl ImageMeasurer for ReaderMeasurer<'_> {
    fn image_dimensions(&mut self, image: &ChapterImage) -> Option<ImageDimensions> {
        self.images.dimensions(image.path())
    }
}
