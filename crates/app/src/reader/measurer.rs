use inkpaper_epub::FontWeight as ReaderFontWeight;
use inkpaper_reader::{ImageMeasurer, TextMeasurer, TextStyle as ReaderTextStyle};
use inkpaper_ui::{
    FontFamilyId, FontRegistry, FontRegistryError, FontWeight as UiFontWeight, ResolvedFont,
    ShapeError, ShapedGlyph, SimpleShaper,
};

const READER_SHAPING_GLYPHS: usize = 128;

pub(super) struct ReaderMeasurer {
    fonts: FontRegistry<'static, 1>,
    glyphs: [ShapedGlyph; READER_SHAPING_GLYPHS],
}

impl ReaderMeasurer {
    pub(super) fn new() -> Result<Self, FontRegistryError> {
        let mut fonts = FontRegistry::default();

        let family = fonts.register_family()?;

        fonts.register_face(family, crate::typography::ui_font())?;

        Ok(Self {
            fonts,
            glyphs: [ShapedGlyph::EMPTY; READER_SHAPING_GLYPHS],
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
        let font = self.resolve_font(style);

        let shaper = SimpleShaper::with_properties(font.properties());

        let summary = shaper.measure(
            &self.fonts,
            font.id(),
            style.font_size(),
            text,
            &mut self.glyphs,
        )?;

        Ok(u32::try_from(summary.advance().non_negative().get()).unwrap_or(u32::MAX))
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
        let font = self.resolve_font(style);

        let shaper = SimpleShaper::with_properties(font.properties());

        Ok(shaper.next_cluster_boundary(&self.fonts, font.id(), style.font_size(), text, from))
    }
}

impl ImageMeasurer for ReaderMeasurer {}
