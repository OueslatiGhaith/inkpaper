use inkpaper_trace::{TraceEvent, profile_expr};

use crate::{
    FontId, FontInstance, FontProperties, FontRegistry, GlyphId, Offset, Pixels, PreparedFont,
};

use self::arabic::MarkPlacement;

mod arabic;
mod bidi;
mod logical;
pub(crate) mod pair_cache;
mod positioning;
#[cfg(test)]
mod tests;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum TextDirection {
    #[default]
    LeftToRight,
    RightToLeft,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct ShapedGlyph {
    font: FontInstance,
    glyph: GlyphId,
    /// UTF-8 byte offset in the original input corresponding to the logical cluster that
    /// produced this glyph.
    ///
    /// multiple glyphs may share one cluster, and one glyph represents multiple
    /// source characters
    cluster: usize,
    /// unpositioned horizontal advance reported by the font.
    ///
    /// this deliberately remains separate from `advance` so bidi reordering can discard
    /// logical-order and rebuild final visual positioning without recovering the value
    /// from `offset`
    base_advance: Pixels,
    /// number of logical source components represented by this base glyph.
    ///
    /// normal bases contain one component. Marks contain zero. Our current lam-alef
    /// ligature contains 2
    ligature_components: u16,
    /// arabic combining-mark role
    mark_placement: Option<MarkPlacement>,
    /// visual position relative to the current pen
    offset: Offset,
    /// amount by which the visual pen moves after this glyph
    advance: Pixels,
}

impl Default for ShapedGlyph {
    fn default() -> Self {
        Self::EMPTY
    }
}

impl ShapedGlyph {
    pub const EMPTY: Self = Self {
        font: FontInstance::DEFAULT,
        glyph: GlyphId::new(0),
        cluster: 0,
        base_advance: Pixels::ZERO,
        ligature_components: 0,
        mark_placement: None,
        offset: Offset::ZERO,
        advance: Pixels::ZERO,
    };

    pub const fn new(
        font: FontId,
        glyph: GlyphId,
        cluster: usize,
        base_advance: Pixels,
        offset: Offset,
        advance: Pixels,
    ) -> Self {
        Self::new_with_instance(
            FontInstance::normal(font),
            glyph,
            cluster,
            base_advance,
            offset,
            advance,
        )
    }

    pub(crate) const fn new_with_instance(
        font: FontInstance,
        glyph: GlyphId,
        cluster: usize,
        base_advance: Pixels,
        offset: Offset,
        advance: Pixels,
    ) -> Self {
        Self {
            font,
            glyph,
            cluster,
            base_advance,
            ligature_components: 1,
            mark_placement: None,
            offset,
            advance,
        }
    }

    const fn new_ligature(
        font: FontId,
        glyph: GlyphId,
        cluster: usize,
        base_advance: Pixels,
        ligature_components: u16,
        offset: Offset,
        advance: Pixels,
    ) -> Self {
        Self::new_ligature_with_instance(
            FontInstance::normal(font),
            glyph,
            cluster,
            base_advance,
            ligature_components,
            offset,
            advance,
        )
    }

    pub(crate) const fn new_ligature_with_instance(
        font: FontInstance,
        glyph: GlyphId,
        cluster: usize,
        base_advance: Pixels,
        ligature_components: u16,
        offset: Offset,
        advance: Pixels,
    ) -> Self {
        Self {
            font,
            glyph,
            cluster,
            base_advance,
            ligature_components,
            mark_placement: None,
            offset,
            advance,
        }
    }

    const fn new_mark(
        font: FontId,
        glyph: GlyphId,
        cluster: usize,
        base_advance: Pixels,
        mark_placement: MarkPlacement,
        offset: Offset,
    ) -> Self {
        Self::new_mark_with_instance(
            FontInstance::normal(font),
            glyph,
            cluster,
            base_advance,
            mark_placement,
            offset,
        )
    }

    pub(crate) const fn new_mark_with_instance(
        font: FontInstance,
        glyph: GlyphId,
        cluster: usize,
        base_advance: Pixels,
        mark_placement: MarkPlacement,
        offset: Offset,
    ) -> Self {
        Self {
            font,
            glyph,
            cluster,
            base_advance,
            ligature_components: 0,
            mark_placement: Some(mark_placement),
            offset,
            advance: Pixels::ZERO,
        }
    }

    const fn with_positioning(self, offset: Offset, advance: Pixels) -> Self {
        Self {
            font: self.font,
            glyph: self.glyph,
            cluster: self.cluster,
            base_advance: self.base_advance,
            ligature_components: self.ligature_components,
            mark_placement: self.mark_placement,
            offset,
            advance,
        }
    }

    pub const fn font(self) -> FontId {
        self.font.font()
    }

    pub const fn font_instance(self) -> FontInstance {
        self.font
    }

    pub const fn glyph(self) -> GlyphId {
        self.glyph
    }

    pub const fn cluster(self) -> usize {
        self.cluster
    }

    pub const fn base_advance(self) -> Pixels {
        self.base_advance
    }

    const fn final_ligature_component(self) -> Option<u16> {
        if self.ligature_components > 1 {
            Some(self.ligature_components - 1)
        } else {
            None
        }
    }

    const fn mark_placement(self) -> Option<MarkPlacement> {
        self.mark_placement
    }

    pub const fn offset(self) -> Offset {
        self.offset
    }

    pub const fn advance(self) -> Pixels {
        self.advance
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct ShapeSummary {
    glyph_count: usize,
    advance: Pixels,
}

impl ShapeSummary {
    pub const fn new(glyph_count: usize, advance: Pixels) -> Self {
        Self {
            glyph_count,
            advance,
        }
    }

    pub const fn glyph_count(self) -> usize {
        self.glyph_count
    }

    pub const fn advance(self) -> Pixels {
        self.advance
    }

    pub const fn is_empty(self) -> bool {
        self.glyph_count == 0
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct ShapeState {
    /// logical joining state is intentionally separate from the previous rendered glyph.
    ///
    /// transparent arabic marks participate in the glyph stream but do not break
    /// contextual joining
    previous_joins_forward: bool,
}

impl ShapeState {
    pub const fn new() -> Self {
        Self {
            previous_joins_forward: false,
        }
    }

    pub fn reset(&mut self) {
        self.previous_joins_forward = false;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ShapeError {
    BufferTooSmall,
    TooManyDirectionalRuns,
}

pub struct ShapedRun<'a> {
    glyphs: &'a [ShapedGlyph],
    direction: TextDirection,
    advance: Pixels,
}

impl<'a> ShapedRun<'a> {
    pub const fn new(glyphs: &'a [ShapedGlyph], direction: TextDirection, advance: Pixels) -> Self {
        Self {
            glyphs,
            direction,
            advance,
        }
    }

    pub const fn glyphs(&self) -> &'a [ShapedGlyph] {
        self.glyphs
    }

    pub const fn direction(&self) -> TextDirection {
        self.direction
    }

    pub const fn advance(&self) -> Pixels {
        self.advance
    }

    pub const fn len(&self) -> usize {
        self.glyphs.len()
    }

    pub const fn is_empty(&self) -> bool {
        self.glyphs.is_empty()
    }
}

/// this is deliberately a very small shaper
///
/// it provides the architecture the renderer will use, but its current shaping rules
/// are only:
/// ```txt
///    Unicode scalar
///      -> font fallbakc
///      -> glyph
///      -> same-face kerning
/// ```
/// arabic joining/bidi will replace these rules without changing [`ShapedGlyph`] or
/// the renderer
#[derive(Debug, Default, Clone, Copy)]
pub struct SimpleShaper {
    properties: FontProperties,
}

impl SimpleShaper {
    pub const fn new() -> Self {
        Self {
            properties: FontProperties::NORMAL,
        }
    }

    pub const fn with_properties(properties: FontProperties) -> Self {
        Self { properties }
    }

    pub(crate) const fn font_instance(self, font: FontId) -> FontInstance {
        FontInstance::new(font, self.properties)
    }

    pub fn measure<'font, const FONTS: usize>(
        &self,
        registry: &FontRegistry<'font, FONTS>,
        preferred_font: FontId,
        size_px: u16,
        text: &str,
        output: &mut [ShapedGlyph],
    ) -> Result<ShapeSummary, ShapeError> {
        self.prepare(registry, preferred_font)
            .measure(registry, size_px, text, output)
    }

    pub fn shape_into<'out, 'font, const FONTS: usize>(
        &self,
        registry: &FontRegistry<'font, FONTS>,
        preferred_font: FontId,
        size_px: u16,
        text: &str,
        output: &'out mut [ShapedGlyph],
    ) -> Result<ShapedRun<'out>, ShapeError> {
        let preferred_font = self.font_instance(preferred_font);
        let prepared_font = registry.prepare_instance(preferred_font);

        let mut state = ShapeState::new();

        let summary = profile_expr!(
            TraceEvent::LogicalShape,
            arg = text.len(),
            self.shape_piece_into_with_prepared_font(
                registry,
                preferred_font,
                prepared_font.as_ref(),
                size_px,
                text,
                &mut state,
                output,
            )?,
        );

        let glyph_count = summary.glyph_count();

        profile_expr!(
            TraceEvent::VisualOrder,
            arg = glyph_count,
            self.visual_order_with_prepared_font(
                registry,
                size_px,
                text,
                glyph_count,
                &mut output[..glyph_count],
                prepared_font.as_ref(),
            ),
        )
    }

    pub(crate) fn shape_into_with_pair_positioning_cache<
        'out,
        'font,
        const FONTS: usize,
        const SLOTS: usize,
    >(
        &self,
        registry: &FontRegistry<'font, FONTS>,
        preferred_font: FontId,
        size_px: u16,
        text: &str,
        output: &'out mut [ShapedGlyph],
        pair_positioning_cache: &mut pair_cache::PairPositioningCache<SLOTS>,
    ) -> Result<ShapedRun<'out>, ShapeError> {
        let preferred_font = self.font_instance(preferred_font);
        let prepared_font = registry.prepare_instance(preferred_font);

        let mut state = ShapeState::new();

        let summary = profile_expr!(
            TraceEvent::LogicalShape,
            arg = text.len(),
            self.shape_piece_into_with_prepared_font(
                registry,
                preferred_font,
                prepared_font.as_ref(),
                size_px,
                text,
                &mut state,
                output,
            )?,
        );

        let glyph_count = summary.glyph_count();

        profile_expr!(
            TraceEvent::VisualOrder,
            arg = glyph_count,
            self.visual_order_with_pair_positioning_cache(
                registry,
                size_px,
                text,
                glyph_count,
                &mut output[..glyph_count],
                prepared_font.as_ref(),
                pair_positioning_cache,
            ),
        )
    }

    pub fn prepare<'font, const FONTS: usize>(
        self,
        registry: &FontRegistry<'font, FONTS>,
        preferred_font: FontId,
    ) -> PreparedSimpleShaper<'font> {
        let preferred_font = self.font_instance(preferred_font);
        let prepared_font = registry.prepare_instance(preferred_font);

        PreparedSimpleShaper {
            shaper: self,
            preferred_font,
            prepared_font,
        }
    }
}

pub struct PreparedSimpleShaper<'font> {
    shaper: SimpleShaper,
    preferred_font: FontInstance,
    prepared_font: Option<PreparedFont<'font>>,
}

impl<'font> PreparedSimpleShaper<'font> {
    pub fn measure<const FONTS: usize>(
        &self,
        registry: &FontRegistry<'font, FONTS>,
        size_px: u16,
        text: &str,
        output: &mut [ShapedGlyph],
    ) -> Result<ShapeSummary, ShapeError> {
        let run = self.shape_into(registry, size_px, text, output)?;

        Ok(ShapeSummary::new(run.len(), run.advance()))
    }

    pub fn shape_into<'out, const FONTS: usize>(
        &self,
        registry: &FontRegistry<'font, FONTS>,
        size_px: u16,
        text: &str,
        output: &'out mut [ShapedGlyph],
    ) -> Result<ShapedRun<'out>, ShapeError> {
        let mut state = ShapeState::new();

        let summary = profile_expr!(
            TraceEvent::LogicalShape,
            arg = text.len(),
            self.shaper.shape_piece_into_with_prepared_font(
                registry,
                self.preferred_font,
                self.prepared_font.as_ref(),
                size_px,
                text,
                &mut state,
                output,
            )?,
        );

        let glyph_count = summary.glyph_count();

        profile_expr!(
            TraceEvent::VisualOrder,
            arg = glyph_count,
            self.shaper.visual_order_with_prepared_font(
                registry,
                size_px,
                text,
                glyph_count,
                &mut output[..glyph_count],
                self.prepared_font.as_ref(),
            ),
        )
    }

    pub fn next_cluster_boundary<const FONTS: usize>(
        &self,
        registry: &FontRegistry<'font, FONTS>,
        size_px: u16,
        text: &str,
        from: usize,
    ) -> Option<usize> {
        self.shaper.next_cluster_boundary_with_prepared_font(
            registry,
            self.preferred_font,
            self.prepared_font.as_ref(),
            size_px,
            text,
            from,
        )
    }
}
