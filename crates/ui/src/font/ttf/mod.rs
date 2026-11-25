use skrifa::{
    FontRef, GlyphId as SkrifaGlyphId, MetadataProvider,
    attribute::{Attributes, Style},
    charmap::Charmap,
    instance::{Location, Size},
    outline::DrawSettings,
};

use crate::{
    FillRule, FontData, FontFace, FontMetrics, FontProperties, FontRasterError, FontStyle,
    FontWeight, FontWeightRange, GlyphId, GlyphMetrics, Pixels, PreparedFontSource,
    raster::{CoverageGrid, GlyphOutlinePen, STACK_CELLS},
};

#[cfg(feature = "hinting")]
mod hinted;
mod metrics;
mod owned;

use metrics::{font_metrics_for_font, glyph_advance_for_font, glyph_metrics_for_font};

#[cfg(feature = "hinting")]
pub use hinted::{HintedOwnedTtfFont, HintedTtfFont};
pub use owned::OwnedTtfFont;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum TtfFontError {
    InvalidFont,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct TtfFont<'a> {
    data: FontData<'a>,
    face_index: u32,
}

impl<'a> TtfFont<'a> {
    pub const fn from_data(data: FontData<'a>, face_index: u32) -> Self {
        Self { data, face_index }
    }

    pub fn parse(data: FontData<'a>, face_index: u32) -> Result<Self, TtfFontError> {
        FontRef::from_index(data.bytes(), face_index).map_err(|_| TtfFontError::InvalidFont)?;

        Ok(Self::from_data(data, face_index))
    }

    pub const fn data(self) -> FontData<'a> {
        self.data
    }

    pub const fn face_index(self) -> u32 {
        self.face_index
    }

    pub(super) fn font_ref(&self) -> Result<FontRef<'a>, TtfFontError> {
        FontRef::from_index(self.data.bytes(), self.face_index)
            .map_err(|_| TtfFontError::InvalidFont)
    }

    pub(super) fn location_for(font: &FontRef<'_>, properties: FontProperties) -> Location {
        #[cfg(feature = "variable-fonts")]
        {
            font.axes()
                .location([("wght", f32::from(properties.weight().value()))])
        }

        #[cfg(not(feature = "variable-fonts"))]
        {
            let _ = (font, properties);
            Location::default()
        }
    }
}

impl FontFace for TtfFont<'_> {
    fn weight_range(&self) -> FontWeightRange {
        let Ok(font) = self.font_ref() else {
            return FontWeightRange::default();
        };

        #[cfg(feature = "variable-fonts")]
        {
            use skrifa::Tag;

            if let Some(axis) = font.axes().get_by_tag(Tag::new(b"wght")) {
                return FontWeightRange::new(
                    font_weight_from_f32(axis.min_value()),
                    font_weight_from_f32(axis.default_value()),
                    font_weight_from_f32(axis.max_value()),
                );
            }
        }

        FontWeightRange::exact(font_weight_from_f32(Attributes::new(&font).weight.value()))
    }

    fn style(&self) -> FontStyle {
        match self.font_ref() {
            Ok(font) => match Attributes::new(&font).style {
                Style::Italic | Style::Oblique(_) => FontStyle::Italic,
                Style::Normal => FontStyle::Normal,
            },
            Err(_) => FontStyle::Normal,
        }
    }

    fn prepare(&self, properties: FontProperties) -> PreparedFontSource<'_> {
        match self.font_ref() {
            Ok(font) => {
                let location = Self::location_for(&font, properties);
                let harfrust = self.harfrust_font(properties);
                PreparedFontSource::from_ttf(PreparedTtfFont::new(font, location, harfrust))
            }
            Err(_) => PreparedFontSource::unprepared(),
        }
    }

    fn is_ttf_shaping(&self) -> bool {
        true
    }

    fn harfrust_font(&self, properties: FontProperties) -> Option<harfrust::Font> {
        let blob = harfrust::font::Blob::from(self.data.bytes().to_vec());
        let base = harfrust::Font::new(blob, self.face_index)?;

        Some(apply_harfrust_properties(&base, properties))
    }

    fn glyph_id(&self, character: char) -> Option<GlyphId> {
        let font = self.font_ref().ok()?;
        let glyph = font.charmap().map(character)?;

        from_skrifa_glyph(glyph)
    }

    fn metrics(&self, properties: FontProperties, size_px: u16) -> FontMetrics {
        let Ok(font) = self.font_ref() else {
            return FontMetrics::default();
        };
        let location = Self::location_for(&font, properties);

        font_metrics_for_font(&font, (&location).into(), size_px)
    }

    fn glyph_advance(
        &self,
        properties: FontProperties,
        glyph: GlyphId,
        size_px: u16,
    ) -> Option<Pixels> {
        let font = self.font_ref().ok()?;
        let location = Self::location_for(&font, properties);

        glyph_advance_for_font(&font, (&location).into(), to_skrifa_glyph(glyph), size_px)
    }

    fn glyph_id_and_advance(
        &self,
        properties: FontProperties,
        character: char,
        size_px: u16,
    ) -> Option<(GlyphId, Pixels)> {
        let font = self.font_ref().ok()?;
        let location = Self::location_for(&font, properties);
        let glyph = font.charmap().map(character)?;
        let glyph_id = from_skrifa_glyph(glyph)?;
        let advance = glyph_advance_for_font(&font, (&location).into(), glyph, size_px)?;

        Some((glyph_id, advance))
    }

    fn glyph_metrics(
        &self,
        properties: FontProperties,
        glyph: GlyphId,
        size_px: u16,
    ) -> Option<GlyphMetrics> {
        let font = self.font_ref().ok()?;
        let location = Self::location_for(&font, properties);

        glyph_metrics_for_font(&font, (&location).into(), to_skrifa_glyph(glyph), size_px)
    }

    fn rasterize(
        &self,
        properties: FontProperties,
        glyph: GlyphId,
        size_px: u16,
        coverage: &mut [u8],
    ) -> Result<(), FontRasterError> {
        let font = self.font_ref().map_err(|_| FontRasterError::InvalidFont)?;
        let location = Self::location_for(&font, properties);

        rasterize_font(&font, &location, glyph, size_px, coverage)
    }
}

pub(super) struct PreparedTtfFont<'a> {
    font: FontRef<'a>,
    charmap: Charmap<'a>,
    location: Location,
    harfrust: Option<harfrust::Font>,
}

impl<'a> PreparedTtfFont<'a> {
    pub(super) fn new(
        font: FontRef<'a>,
        location: Location,
        harfrust: Option<harfrust::Font>,
    ) -> Self {
        let charmap = font.charmap();

        Self {
            font,
            charmap,
            location,
            harfrust,
        }
    }

    pub(super) fn glyph_id(&self, character: char) -> Option<GlyphId> {
        from_skrifa_glyph(self.charmap.map(character)?)
    }

    pub(super) fn harfrust_font(&self) -> Option<&harfrust::Font> {
        self.harfrust.as_ref()
    }

    pub(super) fn glyph_id_and_advance(
        &self,
        character: char,
        size_px: u16,
    ) -> Option<(GlyphId, Pixels)> {
        let glyph = self.charmap.map(character)?;
        let glyph_id = from_skrifa_glyph(glyph)?;
        let advance = glyph_advance_for_font(&self.font, (&self.location).into(), glyph, size_px)?;

        Some((glyph_id, advance))
    }
}

pub(super) fn apply_harfrust_properties(
    base: &harfrust::Font,
    properties: FontProperties,
) -> harfrust::Font {
    #[cfg(feature = "variable-fonts")]
    {
        if !base.axes().is_empty() {
            return base
                .instance_builder()
                .variations([("wght", f32::from(properties.weight().value()))])
                .build();
        }
    }

    #[cfg(not(feature = "variable-fonts"))]
    let _ = properties;

    base.clone()
}

pub(super) fn to_skrifa_glyph(glyph: GlyphId) -> SkrifaGlyphId {
    SkrifaGlyphId::new(u32::from(glyph.value()))
}

fn from_skrifa_glyph(glyph: SkrifaGlyphId) -> Option<GlyphId> {
    u16::try_from(glyph.to_u32()).ok().map(GlyphId::new)
}

fn rasterize_font(
    font: &FontRef<'_>,
    location: &Location,
    glyph: GlyphId,
    size_px: u16,
    coverage: &mut [u8],
) -> Result<(), FontRasterError> {
    if size_px == 0 {
        return Err(FontRasterError::InvalidSize);
    }

    let glyph = to_skrifa_glyph(glyph);

    let metrics = glyph_metrics_for_font(font, location.into(), glyph, size_px)
        .ok_or(FontRasterError::InvalidGlyph)?;

    let required = metrics
        .coverage_bytes()
        .ok_or(FontRasterError::InvalidGlyph)?;

    if coverage.len() < required {
        return Err(FontRasterError::BufferTooSmall);
    }

    coverage[..required].fill(0);

    if required == 0 {
        return Ok(());
    }

    let outline = font
        .outline_glyphs()
        .get(glyph)
        .ok_or(FontRasterError::Unsupported)?;
    let size = Size::new(f32::from(size_px));

    let width = usize::from(metrics.width);
    let height = usize::from(metrics.height);
    let left = metrics.bearing_x.get();
    let top = metrics.bearing_y.get();
    let cells_len = width
        .checked_add(1)
        .and_then(|stride| stride.checked_mul(height))
        .ok_or(FontRasterError::InvalidGlyph)?;

    if cells_len <= STACK_CELLS {
        let mut cells = [0.0f32; STACK_CELLS];
        rasterize_outline_into(
            &mut cells,
            &outline,
            size,
            location,
            width,
            height,
            left,
            top,
            &mut coverage[..required],
        )
    } else {
        let mut cells = alloc::vec![0.0f32; cells_len];
        rasterize_outline_into(
            &mut cells,
            &outline,
            size,
            location,
            width,
            height,
            left,
            top,
            &mut coverage[..required],
        )
    }
}

#[allow(clippy::too_many_arguments)]
fn rasterize_outline_into(
    cells: &mut [f32],
    outline: &skrifa::outline::OutlineGlyph<'_>,
    size: Size,
    location: &Location,
    width: usize,
    height: usize,
    left: i32,
    top: i32,
    coverage: &mut [u8],
) -> Result<(), FontRasterError> {
    let mut grid = CoverageGrid::new(cells, width, height, left, top);
    let mut pen = GlyphOutlinePen::new(&mut grid);

    outline
        .draw(DrawSettings::unhinted(size, location), &mut pen)
        .map_err(|_| FontRasterError::Unsupported)?;
    pen.finish();

    grid.write_coverage(FillRule::NonZero, coverage);

    Ok(())
}

fn font_weight_from_f32(value: f32) -> FontWeight {
    if !value.is_finite() {
        return FontWeight::NORMAL;
    }

    let value = value.clamp(1.0, 1000.0);

    u16::try_from(crate::float::round_i32(value)).map_or(FontWeight::NORMAL, FontWeight::new)
}
