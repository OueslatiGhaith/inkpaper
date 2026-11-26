use alloc::{boxed::Box, vec, vec::Vec};
use core::{fmt, ops::Range};

use inkpaper_epub::{
    ChapterImage, FontStyle as ReaderFontStyle, FontWeight as ReaderFontWeight, ImageDimensions,
};
use inkpaper_reader::{ImageMeasurer, TextMeasurer, TextStyle as ReaderTextStyle};
use inkpaper_ui::{
    FontFamilyId, FontRegistry, FontRegistryError, PreparedSimpleShaper, ResolvedFont, ShapeError,
    SimpleShaper,
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

    /// Measures the visual `(left, right)` x-bounds (relative to the start of
    /// `text`) of each sorted, non-overlapping byte `range` in `ranges` using a
    /// single shaping pass over `text`.
    pub(super) fn measure_ranges(
        &self,
        text: &str,
        style: ReaderTextStyle,
        word_spacing: u32,
        ranges: &[Range<usize>],
    ) -> Result<Vec<Option<(u32, u32)>>, ShapeError> {
        let mut bounds: Vec<Option<(u32, u32)>> = vec![None; ranges.len()];
        if ranges.is_empty() {
            return Ok(bounds);
        }

        let font = resolve_font(self, style.font_weight(), style.font_style());
        let shaper = SimpleShaper::with_properties(font.properties())
            .prepare(&self.registry, font.id());
        let mut pen_x: i32 = 0;

        shaper.shape_line_with(&self.registry, style.font_size(), text, "", |glyph| {
            let start_x = pen_x.max(0) as u32;
            let advance = glyph.advance().get();
            let extra = if text.as_bytes().get(glyph.cluster()) == Some(&b' ') {
                i32::try_from(word_spacing).unwrap_or(i32::MAX)
            } else {
                0
            };
            pen_x = pen_x.saturating_add(advance).saturating_add(extra);
            let end_x = (start_x as i64 + i64::from(advance)).max(0) as u32;

            let cluster = glyph.cluster();
            let index = ranges.partition_point(|range| range.end <= cluster);
            if let Some(range) = ranges.get(index)
                && cluster >= range.start
            {
                bounds[index] = Some(match bounds[index] {
                    Some((min_left, max_right)) => (min_left.min(start_x), max_right.max(end_x)),
                    None => (start_x, end_x),
                });
            }
        })?;

        Ok(bounds)
    }
}

/// Keeps a [`ReaderFonts`] registry cached by card family id so repeated
/// selection drags or settings previews avoid rebuilding fonts on every frame.
#[derive(Default)]
pub(crate) struct CachedReaderFonts {
    cached: Option<(Option<u8>, Box<ReaderFonts>)>,
}

impl CachedReaderFonts {
    pub(crate) fn get_or_init(
        &mut self,
        card: Option<&CardFont>,
    ) -> Result<&ReaderFonts, FontRegistryError> {
        let key = card.map(CardFont::family);
        if self
            .cached
            .as_ref()
            .is_none_or(|(cached_key, _)| *cached_key != key)
        {
            self.cached = Some((key, Box::new(ReaderFonts::new(card)?)));
        }

        Ok(self.cached.as_ref().expect("just initialized").1.as_ref())
    }

    pub(crate) fn clear(&mut self) {
        self.cached = None;
    }
}

impl fmt::Debug for CachedReaderFonts {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CachedReaderFonts")
            .field("card_family", &self.cached.as_ref().map(|(key, _)| *key))
            .finish()
    }
}

/// Measures with shapers prepared from `fonts`, which it borrows since what
/// a registry resolves borrows the registry.
pub(super) struct ReaderMeasurer<'fonts> {
    fonts: &'fonts ReaderFonts,

    /// indexed by [`shaper_index`]
    shapers: [PreparedSimpleShaper<'fonts>; 4],

    images: ChapterImageMetrics,

    measure_cache:
        Box<ReaderMeasureCache<READER_MEASURE_CACHE_SLOTS, READER_MEASURE_CACHE_TEXT_BYTES>>,
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
            images,
            measure_cache: Box::default(),
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
        let insertion = match self.measure_cache.lookup(text, style) {
            ReaderMeasureCacheLookup::Hit(width) => {
                return Ok(width);
            }

            ReaderMeasureCacheLookup::Miss { slot, hash, .. } => Some((slot, hash)),

            ReaderMeasureCacheLookup::Bypass => None,
        };

        let shaper = &self.shapers[shaper_index(style.font_weight(), style.font_style())];

        let summary =
            shaper.shape_line_with(&self.fonts.registry, style.font_size(), text, "", |_| {})?;

        let width = u32::try_from(summary.advance().non_negative().get()).unwrap_or(u32::MAX);

        if let Some((slot, hash)) = insertion {
            self.measure_cache.insert(slot, hash, text, style, width);
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
