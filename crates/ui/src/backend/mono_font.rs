use core::convert::Infallible;

use embedded_graphics::{
    Pixel as EgPixel,
    draw_target::DrawTarget as EgDrawTarget,
    geometry::{OriginDimensions, Point as EgPoint, Size as EgSize},
    image::ImageDrawable as EgImageDrawable,
    mono_font::MonoFont as EgMonoFont,
    pixelcolor::BinaryColor,
    primitives::Rectangle as EgRectangle,
};

use crate::{FontFace, FontMetrics, FontRasterError, GlyphId, GlyphMetrics, Pixels, px};

type CharacterSupport = fn(char) -> bool;

fn all_characters(_: char) -> bool {
    true
}

fn printable_ascii(character: char) -> bool {
    (' '..='~').contains(&character)
}

#[derive(Clone, Copy)]
pub struct MonoFontFace<'font> {
    font: &'font EgMonoFont<'font>,
    supports: CharacterSupport,
}

impl<'font> MonoFontFace<'font> {
    pub const fn new(font: &'font EgMonoFont<'font>) -> Self {
        Self {
            font,
            supports: all_characters,
        }
    }

    pub const fn with_character_support(
        font: &'font EgMonoFont<'font>,
        supports: CharacterSupport,
    ) -> Self {
        Self { font, supports }
    }

    pub const fn ascii(font: &'font EgMonoFont<'font>) -> Self {
        Self::with_character_support(font, printable_ascii)
    }

    pub const fn font(self) -> &'font EgMonoFont<'font> {
        self.font
    }

    pub fn glyph_origin(&self, glyph: GlyphId) -> Option<EgPoint> {
        let character_size = self.font.character_size;
        if character_size.width == 0 || character_size.height == 0 {
            return None;
        }

        let image_size = self.font.image.size();
        let columns = image_size.width / character_size.width;
        let rows = image_size.height / character_size.height;

        if columns == 0 || rows == 0 {
            return None;
        }

        let glyph_index = u32::from(glyph.value());
        let glyph_count = columns.checked_mul(rows)?;

        if glyph_index >= glyph_count {
            return None;
        }

        let column = glyph_index % columns;
        let row = glyph_index / columns;
        let x = column.checked_mul(character_size.width)?;
        let y = row.checked_mul(character_size.height)?;

        Some(EgPoint::new(i32::try_from(x).ok()?, i32::try_from(y).ok()?))
    }
}

impl FontFace for MonoFontFace<'_> {
    fn glyph_id(&self, character: char) -> Option<GlyphId> {
        if !(self.supports)(character) {
            return None;
        }

        let index = self.font.glyph_mapping.index(character);
        let index = u16::try_from(index).ok()?;
        let glyph = GlyphId::new(index);

        self.glyph_origin(glyph)?;

        Some(glyph)
    }

    fn metrics(&self, _: u16) -> FontMetrics {
        let height = pixels_from_u32(self.font.character_size.height);
        let baseline = self.font.baseline.min(self.font.character_size.height);
        let ascent = pixels_from_u32(baseline);
        let descent = height - ascent;

        FontMetrics::new(ascent, descent, Pixels::ZERO)
    }

    fn glyph_metrics(&self, glyph: GlyphId, _: u16) -> Option<GlyphMetrics> {
        self.glyph_origin(glyph)?;

        let width = u16::try_from(self.font.character_size.width).ok()?;
        let height = u16::try_from(self.font.character_size.height).ok()?;
        let baseline = i32::try_from(self.font.baseline).unwrap_or(i32::MAX);

        Some(GlyphMetrics::new(
            width,
            height,
            Pixels::ZERO,
            px(baseline.saturating_neg()),
            pixels_from_u32(self.font.character_size.width),
        ))
    }

    fn kerning(&self, _: GlyphId, _: GlyphId, _: u16) -> Pixels {
        // embedded-graphics models character spacing separately from the glyph width.
        // treating it as pair adjustment gives our generic text engine exactly:
        //      n * glyph_width + (n - 1) * spacing
        // which preserves the old MonoFont measurement
        pixels_from_u32(self.font.character_spacing)
    }

    fn rasterize(
        &self,
        glyph: GlyphId,
        _: u16,
        coverage: &mut [u8],
    ) -> Result<(), FontRasterError> {
        let metrics = self
            .glyph_metrics(glyph, 0)
            .ok_or(FontRasterError::InvalidGlyph)?;
        let required = metrics
            .coverage_bytes()
            .ok_or(FontRasterError::InvalidGlyph)?;

        if coverage.len() < required {
            return Err(FontRasterError::BufferTooSmall);
        }
        if required == 0 {
            return Ok(());
        }

        let origin = self
            .glyph_origin(glyph)
            .ok_or(FontRasterError::InvalidGlyph)?;

        coverage[..required].fill(0);

        let mut target = CoverageTarget::new(&mut coverage[..required], self.font.character_size);
        let source = EgRectangle::new(origin, self.font.character_size);

        match self.font.image.draw_sub_image(&mut target, &source) {
            Ok(()) => Ok(()),
            Err(error) => match error {},
        }
    }
}

fn pixels_from_u32(value: u32) -> Pixels {
    px(i32::try_from(value).unwrap_or(i32::MAX))
}

struct CoverageTarget<'a> {
    coverage: &'a mut [u8],
    size: EgSize,
}

impl<'a> CoverageTarget<'a> {
    fn new(coverage: &'a mut [u8], size: EgSize) -> Self {
        Self { coverage, size }
    }
}

impl OriginDimensions for CoverageTarget<'_> {
    fn size(&self) -> EgSize {
        self.size
    }
}

impl EgDrawTarget for CoverageTarget<'_> {
    type Color = BinaryColor;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = EgPixel<Self::Color>>,
    {
        for EgPixel(point, color) in pixels {
            if point.x < 0 || point.y < 0 {
                continue;
            }
            let Ok(x) = usize::try_from(point.x) else {
                continue;
            };
            let Ok(y) = usize::try_from(point.y) else {
                continue;
            };

            let width = self.size.width as usize;
            let height = self.size.height as usize;

            if x >= width || y >= height {
                continue;
            }

            let Some(index) = y.checked_mul(width).and_then(|row| row.checked_add(x)) else {
                continue;
            };
            let Some(destination) = self.coverage.get_mut(index) else {
                continue;
            };

            *destination = if color.is_on() { 255 } else { 0 };
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use embedded_graphics::mono_font::ascii::FONT_6X10;

    use super::*;

    #[test]
    fn mono_font_face_preserves_native_metrics() {
        let font = MonoFontFace::new(&FONT_6X10);
        let glyph = font.glyph_id('A').unwrap();
        let metrics = font.glyph_metrics(glyph, 0).unwrap();

        assert_eq!(metrics.width, 6,);
        assert_eq!(metrics.height, 10,);
        assert_eq!(metrics.advance, px(6),);
        assert_eq!(font.metrics(0).line_height(), px(10),);
    }

    #[test]
    fn mono_font_face_rasterizes_into_caller_buffer() {
        let font = MonoFontFace::new(&FONT_6X10);
        let glyph = font.glyph_id('A').unwrap();
        let metrics = font.glyph_metrics(glyph, 0).unwrap();
        let required = metrics.coverage_bytes().unwrap();
        let mut coverage = [0u8; 128];

        font.rasterize(glyph, 0, &mut coverage[..required]).unwrap();

        assert!(coverage[..required].iter().any(|value| { *value == 255 },),);
        assert!(coverage[..required].iter().any(|value| { *value == 0 },),);
    }

    #[test]
    fn ascii_face_reports_non_ascii_as_missing() {
        let font = MonoFontFace::ascii(&FONT_6X10);

        assert!(font.glyph_id('A',).is_some(),);
        assert!(font.glyph_id('ب',).is_none(),);
    }
}
