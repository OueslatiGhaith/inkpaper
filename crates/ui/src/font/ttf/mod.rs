use skrifa::{
    FontRef, GlyphId as SkrifaGlyphId, GlyphId16, MetadataProvider,
    attribute::{Attributes, Style},
    charmap::Charmap,
    instance::{Location, Size},
    outline::DrawSettings,
};

use crate::{
    CursiveAttachment, FontData, FontFace, FontMetrics, FontProperties, FontRasterError, FontStyle,
    FontWeight, FontWeightRange, GlyphId, GlyphMetrics, Offset, OpenTypeFeature, PairPositioning,
    Pixels, PreparedFontSource, px,
};

mod gpos;
mod gsub;
#[cfg(feature = "hinting")]
mod hinted;
mod metrics;
mod owned;
mod raster;

use gpos::{
    gpos_cursive_attachment_for_font, gpos_kerning_for_font, gpos_pair_positioning_for_font,
    legacy_kerning_for_font, mark_to_base_offset_for_font, mark_to_ligature_offset_for_font,
    mark_to_mark_offset_for_font,
};
use gsub::{gsub_pair_ligature_for_font, gsub_single_substitution_for_font};
use metrics::{font_metrics_for_font, glyph_advance_for_font, glyph_metrics_for_font};
use raster::{SUPERSAMPLE_Y, ScanlineBuilder, accumulate_scanline, normalize_coverage};

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
                PreparedFontSource::from_ttf(PreparedTtfFont::new(font, location))
            }
            Err(_) => PreparedFontSource::unprepared(),
        }
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

    fn kerning(
        &self,
        _properties: FontProperties,
        left: GlyphId,
        right: GlyphId,
        size_px: u16,
    ) -> Pixels {
        let Ok(font) = self.font_ref() else {
            return px(0);
        };

        let left = to_glyph16(left);
        let right = to_glyph16(right);

        if let Some(adjustment) = gpos_kerning_for_font(&font, left, right, size_px) {
            return adjustment;
        }

        legacy_kerning_for_font(&font, left, right, size_px).unwrap_or(px(0))
    }

    fn single_substitution(
        &self,
        _properties: FontProperties,
        feature: OpenTypeFeature,
        glyph: GlyphId,
    ) -> Option<GlyphId> {
        let font = self.font_ref().ok()?;
        let substituted = gsub_single_substitution_for_font(&font, feature, to_glyph16(glyph))?;

        Some(GlyphId::new(substituted.to_u16()))
    }

    fn ligature_substitution(
        &self,
        _properties: FontProperties,
        feature: OpenTypeFeature,
        first: GlyphId,
        second: GlyphId,
    ) -> Option<GlyphId> {
        let font = self.font_ref().ok()?;
        let substituted =
            gsub_pair_ligature_for_font(&font, feature, to_glyph16(first), to_glyph16(second))?;

        Some(GlyphId::new(substituted.to_u16()))
    }

    fn cursive_attachment(
        &self,
        _properties: FontProperties,
        visual_left: GlyphId,
        visual_right: GlyphId,
        size_px: u16,
        right_to_left: bool,
    ) -> Option<CursiveAttachment> {
        let font = self.font_ref().ok()?;

        gpos_cursive_attachment_for_font(
            &font,
            to_glyph16(visual_left),
            to_glyph16(visual_right),
            size_px,
            right_to_left,
        )
    }

    fn pair_positioning(
        &self,
        _properties: FontProperties,
        visual_left: GlyphId,
        visual_right: GlyphId,
        size_px: u16,
        right_to_left: bool,
    ) -> PairPositioning {
        let Ok(font) = self.font_ref() else {
            return PairPositioning::Kerning(px(0));
        };

        pair_positioning_for_font(&font, visual_left, visual_right, size_px, right_to_left)
    }

    fn mark_to_base_offset(
        &self,
        _properties: FontProperties,
        base: GlyphId,
        mark: GlyphId,
        size_px: u16,
    ) -> Option<Offset> {
        let font = self.font_ref().ok()?;

        mark_to_base_offset_for_font(&font, to_glyph16(base), to_glyph16(mark), size_px)
    }

    fn mark_to_ligature_offset(
        &self,
        _properties: FontProperties,
        ligature: GlyphId,
        component: u16,
        mark: GlyphId,
        size_px: u16,
    ) -> Option<Offset> {
        let font = self.font_ref().ok()?;

        mark_to_ligature_offset_for_font(
            &font,
            to_glyph16(ligature),
            component,
            to_glyph16(mark),
            size_px,
        )
    }

    fn mark_to_mark_offset(
        &self,
        _properties: FontProperties,
        base_mark: GlyphId,
        mark: GlyphId,
        size_px: u16,
    ) -> Option<Offset> {
        let font = self.font_ref().ok()?;

        mark_to_mark_offset_for_font(&font, to_glyph16(base_mark), to_glyph16(mark), size_px)
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
}

impl<'a> PreparedTtfFont<'a> {
    fn new(font: FontRef<'a>, location: Location) -> Self {
        let charmap = font.charmap();

        Self {
            font,
            charmap,
            location,
        }
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

    pub(super) fn pair_positioning(
        &self,
        visual_left: GlyphId,
        visual_right: GlyphId,
        size_px: u16,
        right_to_left: bool,
    ) -> PairPositioning {
        pair_positioning_for_font(
            &self.font,
            visual_left,
            visual_right,
            size_px,
            right_to_left,
        )
    }
}

pub(super) fn to_skrifa_glyph(glyph: GlyphId) -> SkrifaGlyphId {
    SkrifaGlyphId::new(u32::from(glyph.value()))
}

fn to_glyph16(glyph: GlyphId) -> GlyphId16 {
    GlyphId16::new(glyph.value())
}

fn from_skrifa_glyph(glyph: SkrifaGlyphId) -> Option<GlyphId> {
    u16::try_from(glyph.to_u32()).ok().map(GlyphId::new)
}

fn pair_positioning_for_font(
    font: &FontRef<'_>,
    visual_left: GlyphId,
    visual_right: GlyphId,
    size_px: u16,
    right_to_left: bool,
) -> PairPositioning {
    let visual_left = to_glyph16(visual_left);
    let visual_right = to_glyph16(visual_right);

    if let Some(positioning) =
        gpos_pair_positioning_for_font(font, visual_left, visual_right, size_px, right_to_left)
    {
        return positioning;
    }

    let kerning = legacy_kerning_for_font(font, visual_left, visual_right, size_px);

    PairPositioning::Kerning(kerning.unwrap_or(px(0)))
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

    for row in 0..height {
        for sub_y in 0..SUPERSAMPLE_Y {
            let sample_y = row as f32 + (sub_y as f32 + 0.5) / SUPERSAMPLE_Y as f32;

            let mut scanline = ScanlineBuilder::new(1.0, left, top, sample_y);

            outline
                .draw(DrawSettings::unhinted(size, location), &mut scanline)
                .map_err(|_| FontRasterError::Unsupported)?;

            if scanline.overflowed() {
                return Err(FontRasterError::OutlineTooComplex);
            }

            scanline.sort_intersections();

            accumulate_scanline(
                width,
                row,
                &mut coverage[..required],
                scanline.intersections(),
            );
        }
    }

    normalize_coverage(&mut coverage[..required]);

    Ok(())
}

fn font_weight_from_f32(value: f32) -> FontWeight {
    if !value.is_finite() {
        return FontWeight::NORMAL;
    }

    let value = value.clamp(1.0, 1000.0);

    u16::try_from(crate::float::round_i32(value)).map_or(FontWeight::NORMAL, FontWeight::new)
}
