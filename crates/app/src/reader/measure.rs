use inkpaper_reader::{
    ChapterImage, ImageDimensions, ImageMeasurer, TextMeasurer as ReaderTextMeasurer,
    TextStyle as ReaderTextStyle,
};
use inkpaper_ui::{FontId, FontRegistry, Pixels, ShapeError, ShapedGlyph, SimpleShaper};

use super::ReaderPageResources;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiReaderMeasureError {
    MissingFont(FontId),
    Shape(ShapeError),
}

impl From<ShapeError> for UiReaderMeasureError {
    fn from(error: ShapeError) -> Self {
        Self::Shape(error)
    }
}

pub struct UiReaderMeasurer<'resource, 'font, R: ?Sized, const FONTS: usize> {
    fonts: FontRegistry<'font, FONTS>,
    resources: &'resource R,
    scratch: &'resource mut [ShapedGlyph],
}

impl<'resource, 'font, R, const FONTS: usize> UiReaderMeasurer<'resource, 'font, R, FONTS>
where
    R: ReaderPageResources + ?Sized,
{
    pub fn new(
        fonts: FontRegistry<'font, FONTS>,
        resources: &'resource R,
        scratch: &'resource mut [ShapedGlyph],
    ) -> Self {
        Self {
            fonts,
            resources,
            scratch,
        }
    }

    pub const fn scratch_capacity(&self) -> usize {
        self.scratch.len()
    }

    fn resolved_font(
        &self,
        style: ReaderTextStyle,
    ) -> Result<(FontId, &'font dyn inkpaper_ui::FontFace), UiReaderMeasureError> {
        let requested = self.resources.font_for(style);

        self.fonts
            .resolve_with_id(requested)
            .ok_or(UiReaderMeasureError::MissingFont(requested))
    }
}

impl<R, const FONTS: usize> ReaderTextMeasurer for UiReaderMeasurer<'_, '_, R, FONTS>
where
    R: ReaderPageResources + ?Sized,
{
    type Error = UiReaderMeasureError;

    fn measure_text(&mut self, text: &str, style: ReaderTextStyle) -> Result<u32, Self::Error> {
        if text.is_empty() {
            return Ok(0);
        }

        let (font_id, _) = self.resolved_font(style)?;
        let size_px = reader_font_size(style);
        let summary =
            SimpleShaper::new().measure(&self.fonts, font_id, size_px, text, self.scratch)?;

        Ok(reader_pixels_to_u32(summary.advance()))
    }

    fn line_height(&mut self, style: ReaderTextStyle) -> Result<u32, Self::Error> {
        let (_, font) = self.resolved_font(style)?;

        let height = font.metrics(reader_font_size(style)).line_height();

        Ok(reader_pixels_to_u32(height))
    }

    fn next_boundary(
        &mut self,
        text: &str,
        from: usize,
        style: ReaderTextStyle,
    ) -> Result<Option<usize>, Self::Error> {
        let (font_id, _) = self.resolved_font(style)?;

        Ok(SimpleShaper::new().next_cluster_boundary(
            &self.fonts,
            font_id,
            reader_font_size(style),
            text,
            from,
        ))
    }
}

impl<R, const FONTS: usize> ImageMeasurer for UiReaderMeasurer<'_, '_, R, FONTS>
where
    R: ReaderPageResources + ?Sized,
{
    fn image_dimensions(&mut self, image: &ChapterImage) -> Option<ImageDimensions> {
        self.resources.image_dimensions(image)
    }
}

fn reader_font_size(style: ReaderTextStyle) -> u16 {
    style.font_size().max(1)
}

fn reader_pixels_to_u32(value: Pixels) -> u32 {
    u32::try_from(value.non_negative().get()).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use inkpaper_reader::{BlockKind, FontStyle, FontWeight};
    use inkpaper_ui::{FontFace, FontMetrics, FontRasterError, GlyphId, GlyphMetrics, Pixels, px};

    use super::*;

    struct TestFont;

    impl FontFace for TestFont {
        fn glyph_id(&self, _character: char) -> Option<GlyphId> {
            Some(GlyphId::new(0))
        }

        fn metrics(&self, _size_px: u16) -> FontMetrics {
            FontMetrics::new(px(3), px(1), px(1))
        }

        fn glyph_metrics(&self, _glyph: GlyphId, _size_px: u16) -> Option<GlyphMetrics> {
            Some(GlyphMetrics::new(1, 1, Pixels::ZERO, Pixels::ZERO, px(2)))
        }

        fn rasterize(
            &self,
            _glyph: GlyphId,
            _size_px: u16,
            _coverage: &mut [u8],
        ) -> Result<(), FontRasterError> {
            Err(FontRasterError::Unsupported)
        }
    }

    struct TestResources {
        font: FontId,
    }

    impl ReaderPageResources for TestResources {
        fn font_for(&self, _style: ReaderTextStyle) -> FontId {
            self.font
        }
    }

    fn body_style() -> ReaderTextStyle {
        ReaderTextStyle::new(
            16,
            BlockKind::Paragraph,
            FontWeight::Normal,
            FontStyle::Normal,
        )
    }

    #[test]
    fn ui_reader_measurer_uses_ui_font_metrics() {
        let font = TestFont;

        let mut fonts = FontRegistry::<1>::default();
        let font_id = fonts.register(&font).unwrap();
        let resources = TestResources { font: font_id };

        let mut scratch = [ShapedGlyph::EMPTY; 8];
        let mut measurer = UiReaderMeasurer::new(fonts, &resources, &mut scratch);

        assert_eq!(
            ReaderTextMeasurer::measure_text(&mut measurer, "abc", body_style()),
            Ok(6),
        );

        assert_eq!(
            ReaderTextMeasurer::line_height(&mut measurer, body_style()),
            Ok(5),
        );
    }

    #[test]
    fn ui_reader_measurer_uses_shaping_cluster_boundaries() {
        let font = TestFont;

        let mut fonts = FontRegistry::<1>::default();
        let font_id = fonts.register(&font).unwrap();
        let resources = TestResources { font: font_id };

        let mut scratch = [ShapedGlyph::EMPTY; 8];
        let mut measurer = UiReaderMeasurer::new(fonts, &resources, &mut scratch);

        assert_eq!(
            ReaderTextMeasurer::next_boundary(&mut measurer, "abc", 0, body_style()),
            Ok(Some(1)),
        );
    }

    #[test]
    fn ui_reader_measurer_propagates_small_scratch_buffer_error() {
        let font = TestFont;

        let mut fonts = FontRegistry::<1>::default();
        let font_id = fonts.register(&font).unwrap();
        let resources = TestResources { font: font_id };

        let mut scratch = [ShapedGlyph::EMPTY; 1];
        let mut measurer = UiReaderMeasurer::new(fonts, &resources, &mut scratch);

        assert_eq!(
            ReaderTextMeasurer::measure_text(&mut measurer, "ab", body_style()),
            Err(UiReaderMeasureError::Shape(ShapeError::BufferTooSmall)),
        );
    }

    #[test]
    fn ui_reader_measurer_reports_missing_default_font() {
        let fonts = FontRegistry::<1>::default();
        let resources = TestResources {
            font: FontId::DEFAULT,
        };

        let mut scratch = [ShapedGlyph::EMPTY; 8];
        let mut measurer = UiReaderMeasurer::new(fonts, &resources, &mut scratch);

        assert_eq!(
            ReaderTextMeasurer::line_height(&mut measurer, body_style()),
            Err(UiReaderMeasureError::MissingFont(FontId::DEFAULT)),
        );
    }
}
