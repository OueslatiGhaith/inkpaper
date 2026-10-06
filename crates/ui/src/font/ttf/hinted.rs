//! TrueType glyphs snapped to the pixel grid by skrifa's port of FreeType's
//! autohinter, so stems of one size are drawn the same width in every letter.
//!
//! crosspoint gets its crisp text the same way, from FreeType's autohinter. Only the
//! glyph outlines change: advances, kerning and every other metric come from the
//! unhinted font, so line breaking and shaping are unaffected.

use alloc::vec::Vec;

use skrifa::{
    FontRef, MetadataProvider,
    instance::{Location, Size},
    outline::{
        DrawSettings, Engine, GlyphStyles, HintingInstance, HintingOptions, OutlineGlyphCollection,
        OutlinePen, SmoothMode, Target,
    },
};
use ttf_parser::{Face, OutlineBuilder};

use super::{
    TtfFont,
    metrics::glyph_advance_for_face,
    raster::{
        Edge, EdgeBuilder, SUPERSAMPLE_Y, ScanlineBuilder, accumulate_scanline, normalize_coverage,
    },
    to_ttf_glyph,
};
use crate::{
    CursiveAttachment, FontFace, FontMetrics, FontProperties, FontRasterError, FontWeight,
    FontWeightRange, GlyphId, GlyphMetrics, Offset, OpenTypeFeature, PairPositioning, Pixels,
    PreparedFontSource, px,
};

/// the reader and the UI each use a few sizes, and a hinting instance costs a pass
/// over the font's style metrics to build
const CACHED_INSTANCES: usize = 4;

/// one parsed font per weight in use, usually regular and bold
const CACHED_FACES: usize = 2;

/// normal hinting also snaps vertical stems, which is what makes their widths
/// consistent. Light hinting only snaps heights
const TARGET: Target = Target::Smooth {
    mode: SmoothMode::Normal,
    symmetric_rendering: true,
    // skrifa treats preserved linear metrics as light hinting
    preserve_linear_metrics: false,
};

pub struct HintedTtfFont<'a> {
    font: TtfFont<'a>,
    state: spin::Mutex<HintingState<'a>>,
}

struct HintingState<'a> {
    /// opened on first use, so a new glyph doesn't reopen the font
    outlines: Option<OutlineGlyphCollection<'a>>,
    /// parsed on first use, so a new glyph's advance doesn't reparse the font
    faces: Vec<CachedFace<'a>>,
    instances: Instances,
    /// the glyph cache asks for a glyph's metrics right before rasterizing it
    last_outline: Option<CachedOutline>,
}

struct Instances {
    /// computed from the whole font on first use, then shared by every instance
    styles: Option<GlyphStyles>,
    /// most recently used last
    cached: Vec<CachedInstance>,
}

/// the unhinted font the advances come from, with its weight already set
struct CachedFace<'a> {
    weight: Option<FontWeight>,
    face: Face<'a>,
}

struct CachedInstance {
    key: InstanceKey,
    instance: HintingInstance,
}

struct CachedOutline {
    key: InstanceKey,
    glyph: GlyphId,
    outline: HintedOutline,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct InstanceKey {
    size_px: u16,
    /// `None` is the font's default instance
    weight: Option<FontWeight>,
}

impl<'a> HintedTtfFont<'a> {
    pub const fn new(font: TtfFont<'a>) -> Self {
        Self {
            font,
            state: spin::Mutex::new(HintingState {
                outlines: None,
                faces: Vec::new(),
                instances: Instances {
                    styles: None,
                    cached: Vec::new(),
                },
                last_outline: None,
            }),
        }
    }

    pub const fn font(&self) -> &TtfFont<'a> {
        &self.font
    }

    fn glyph_metrics_at(&self, key: InstanceKey, glyph: GlyphId) -> Option<GlyphMetrics> {
        let advance = {
            let mut state = self.state.lock();
            let face = state.face(&self.font, key.weight)?;

            glyph_advance_for_face(face, to_ttf_glyph(glyph), key.size_px)?
        };

        self.with_outline(key, glyph, |outline| outline.metrics(advance))
            .ok()
            .flatten()
    }

    fn rasterize_at(
        &self,
        key: InstanceKey,
        glyph: GlyphId,
        coverage: &mut [u8],
    ) -> Result<(), FontRasterError> {
        self.with_outline(key, glyph, |outline| outline.rasterize(coverage))?
    }

    fn with_outline<R>(
        &self,
        key: InstanceKey,
        glyph: GlyphId,
        use_outline: impl FnOnce(&HintedOutline) -> R,
    ) -> Result<R, FontRasterError> {
        if key.size_px == 0 {
            return Err(FontRasterError::InvalidSize);
        }

        let mut state = self.state.lock();

        if let Some(cached) = &state.last_outline
            && cached.key == key
            && cached.glyph == glyph
        {
            return Ok(use_outline(&cached.outline));
        }

        if state.outlines.is_none() {
            let font = FontRef::from_index(self.font.data().bytes(), self.font.face_index())
                .map_err(|_| FontRasterError::InvalidFont)?;

            state.outlines = Some(font.outline_glyphs());
        }

        let state = &mut *state;
        let outlines = state
            .outlines
            .as_ref()
            .expect("the outlines were just opened");
        let instance = state.instances.get(&self.font, outlines, key)?;

        let mut recorder = OutlineRecorder::default();

        outlines
            .get(skrifa::GlyphId::new(u32::from(glyph.value())))
            .ok_or(FontRasterError::InvalidGlyph)?
            .draw(DrawSettings::hinted(instance, false), &mut recorder)
            .map_err(|_| FontRasterError::Unsupported)?;

        let outline = recorder.finish();
        let result = use_outline(&outline);

        state.last_outline = Some(CachedOutline {
            key,
            glyph,
            outline,
        });

        Ok(result)
    }
}

impl<'a> HintingState<'a> {
    fn face(&mut self, font: &TtfFont<'a>, weight: Option<FontWeight>) -> Option<&Face<'a>> {
        if let Some(index) = self.faces.iter().position(|cached| cached.weight == weight) {
            let cached = self.faces.remove(index);
            self.faces.push(cached);
        } else {
            let face = match weight {
                Some(weight) => font.face_with_properties(FontProperties::new(weight)),
                None => font.face(),
            }
            .ok()?;

            if self.faces.len() == CACHED_FACES {
                self.faces.remove(0);
            }

            self.faces.push(CachedFace { weight, face });
        }

        self.faces.last().map(|cached| &cached.face)
    }
}

impl Instances {
    fn get(
        &mut self,
        font: &TtfFont<'_>,
        outlines: &OutlineGlyphCollection<'_>,
        key: InstanceKey,
    ) -> Result<&HintingInstance, FontRasterError> {
        if let Some(index) = self.cached.iter().position(|cached| cached.key == key) {
            let cached = self.cached.remove(index);
            self.cached.push(cached);
        } else {
            let styles = self
                .styles
                .get_or_insert_with(|| GlyphStyles::new(outlines))
                .clone();

            let instance = HintingInstance::new(
                outlines,
                Size::new(f32::from(key.size_px)),
                &location(font, key.weight)?,
                HintingOptions {
                    engine: Engine::Auto(Some(styles)),
                    target: TARGET,
                },
            )
            .map_err(|_| FontRasterError::Unsupported)?;

            if self.cached.len() == CACHED_INSTANCES {
                self.cached.remove(0);
            }

            self.cached.push(CachedInstance { key, instance });
        }

        Ok(&self
            .cached
            .last()
            .expect("the requested instance was just pushed")
            .instance)
    }
}

/// opens the font only for a new instance, which is rare
#[cfg(feature = "variable-fonts")]
fn location(font: &TtfFont<'_>, weight: Option<FontWeight>) -> Result<Location, FontRasterError> {
    let Some(weight) = weight else {
        return Ok(Location::default());
    };

    let font = FontRef::from_index(font.data().bytes(), font.face_index())
        .map_err(|_| FontRasterError::InvalidFont)?;

    Ok(font.axes().location([("wght", f32::from(weight.value()))]))
}

#[cfg(not(feature = "variable-fonts"))]
fn location(_font: &TtfFont<'_>, _weight: Option<FontWeight>) -> Result<Location, FontRasterError> {
    Ok(Location::default())
}

#[derive(Clone, Copy)]
enum PathCommand {
    MoveTo(f32, f32),
    LineTo(f32, f32),
    QuadTo(f32, f32, f32, f32),
    CurveTo(f32, f32, f32, f32, f32, f32),
    Close,
}

/// a hinted outline in pixels, y up, as the pen draws it
#[derive(Default)]
struct OutlineRecorder {
    commands: Vec<PathCommand>,
    /// left, bottom, right, top of every point, including off-curve points
    bounds: Option<(f32, f32, f32, f32)>,
}

impl OutlineRecorder {
    fn include(&mut self, x: f32, y: f32) {
        self.bounds = Some(match self.bounds {
            None => (x, y, x, y),
            Some((left, bottom, right, top)) => {
                (left.min(x), bottom.min(y), right.max(x), top.max(y))
            }
        });
    }

    /// bitmap bounds in pixels, y down, like `glyph_metrics_for_face`
    fn pixel_bounds(&self) -> Option<(i32, i32, i32, i32)> {
        let (left, bottom, right, top) = self.bounds?;

        let left = libm::floorf(left) as i32;
        let top = libm::floorf(-top) as i32;
        let width = (libm::ceilf(right) as i32).saturating_sub(left).max(0);
        let height = (libm::ceilf(-bottom) as i32).saturating_sub(top).max(0);

        Some((left, top, width, height))
    }

    /// flattens the outline into bitmap space once, so rasterizing it doesn't
    /// redo the curve math for every sample row
    fn finish(self) -> HintedOutline {
        let Some(bounds) = self.pixel_bounds() else {
            return HintedOutline {
                bounds: None,
                edges: Vec::new(),
            };
        };

        let (left, top, _, _) = bounds;
        let mut edges = EdgeBuilder::new(left, top);

        for command in &self.commands {
            match *command {
                PathCommand::MoveTo(x, y) => edges.move_to(x, y),
                PathCommand::LineTo(x, y) => edges.line_to(x, y),
                PathCommand::QuadTo(x1, y1, x, y) => edges.quad_to(x1, y1, x, y),
                PathCommand::CurveTo(x1, y1, x2, y2, x, y) => edges.curve_to(x1, y1, x2, y2, x, y),
                PathCommand::Close => edges.close(),
            }
        }

        HintedOutline {
            bounds: Some(bounds),
            edges: edges.finish(),
        }
    }
}

impl OutlinePen for OutlineRecorder {
    fn move_to(&mut self, x: f32, y: f32) {
        self.include(x, y);
        self.commands.push(PathCommand::MoveTo(x, y));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.include(x, y);
        self.commands.push(PathCommand::LineTo(x, y));
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.include(cx0, cy0);
        self.include(x, y);
        self.commands.push(PathCommand::QuadTo(cx0, cy0, x, y));
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.include(cx0, cy0);
        self.include(cx1, cy1);
        self.include(x, y);
        self.commands
            .push(PathCommand::CurveTo(cx0, cy0, cx1, cy1, x, y));
    }

    fn close(&mut self) {
        self.commands.push(PathCommand::Close);
    }
}

/// a hinted outline flattened into bitmap space
struct HintedOutline {
    /// left, top, width, height in pixels, y down
    bounds: Option<(i32, i32, i32, i32)>,
    /// sorted by top
    edges: Vec<Edge>,
}

impl HintedOutline {
    fn metrics(&self, advance: Pixels) -> Option<GlyphMetrics> {
        let Some((left, top, width, height)) = self.bounds else {
            return Some(GlyphMetrics::new(0, 0, px(0), px(0), advance));
        };

        Some(GlyphMetrics::new(
            u16::try_from(width).ok()?,
            u16::try_from(height).ok()?,
            px(left),
            px(top),
            advance,
        ))
    }

    /// fills the outline exactly like `rasterize_face` fills an unhinted one, but
    /// each sample row only visits the edges that can cross it
    fn rasterize(&self, coverage: &mut [u8]) -> Result<(), FontRasterError> {
        let Some((left, top, width, height)) = self.bounds else {
            return Ok(());
        };

        let width = usize::try_from(width).map_err(|_| FontRasterError::InvalidGlyph)?;
        let height = usize::try_from(height).map_err(|_| FontRasterError::InvalidGlyph)?;
        let required = width
            .checked_mul(height)
            .ok_or(FontRasterError::InvalidGlyph)?;

        let coverage = coverage
            .get_mut(..required)
            .ok_or(FontRasterError::BufferTooSmall)?;

        coverage.fill(0);

        for row in 0..height {
            for sub_y in 0..SUPERSAMPLE_Y {
                let sample_y = row as f32 + (sub_y as f32 + 0.5) / SUPERSAMPLE_Y as f32;

                let mut scanline = ScanlineBuilder::new(1.0, left, top, sample_y);

                for edge in &self.edges {
                    if edge.top > sample_y {
                        break;
                    }

                    if edge.bottom > sample_y {
                        scanline.add_segment(edge.from, edge.to);
                    }
                }

                if scanline.overflowed() {
                    return Err(FontRasterError::OutlineTooComplex);
                }

                scanline.sort_intersections();

                accumulate_scanline(width, row, coverage, scanline.intersections());
            }
        }

        normalize_coverage(coverage);

        Ok(())
    }
}

fn default_instance(size_px: u16) -> InstanceKey {
    InstanceKey {
        size_px,
        weight: None,
    }
}

fn instance_with(properties: FontProperties, size_px: u16) -> InstanceKey {
    InstanceKey {
        size_px,
        weight: Some(properties.weight()),
    }
}

impl FontFace for HintedTtfFont<'_> {
    fn weight_range(&self) -> FontWeightRange {
        self.font.weight_range()
    }

    fn prepare_with_properties(&self, properties: FontProperties) -> PreparedFontSource<'_> {
        self.font.prepare_with_properties(properties)
    }

    fn glyph_id(&self, character: char) -> Option<GlyphId> {
        self.font.glyph_id(character)
    }

    fn metrics(&self, size_px: u16) -> FontMetrics {
        self.font.metrics(size_px)
    }

    fn glyph_advance(&self, glyph: GlyphId, size_px: u16) -> Option<Pixels> {
        self.font.glyph_advance(glyph, size_px)
    }

    fn glyph_metrics(&self, glyph: GlyphId, size_px: u16) -> Option<GlyphMetrics> {
        self.glyph_metrics_at(default_instance(size_px), glyph)
    }

    fn kerning(&self, left: GlyphId, right: GlyphId, size_px: u16) -> Pixels {
        self.font.kerning(left, right, size_px)
    }

    fn single_substitution(&self, feature: OpenTypeFeature, glyph: GlyphId) -> Option<GlyphId> {
        self.font.single_substitution(feature, glyph)
    }

    fn ligature_substitution(
        &self,
        feature: OpenTypeFeature,
        first: GlyphId,
        second: GlyphId,
    ) -> Option<GlyphId> {
        self.font.ligature_substitution(feature, first, second)
    }

    fn cursive_attachment(
        &self,
        visual_left: GlyphId,
        visual_right: GlyphId,
        size_px: u16,
        right_to_left: bool,
    ) -> Option<CursiveAttachment> {
        self.font
            .cursive_attachment(visual_left, visual_right, size_px, right_to_left)
    }

    fn mark_to_base_offset(&self, base: GlyphId, mark: GlyphId, size_px: u16) -> Option<Offset> {
        self.font.mark_to_base_offset(base, mark, size_px)
    }

    fn mark_to_ligature_offset(
        &self,
        ligature: GlyphId,
        component: u16,
        mark: GlyphId,
        size_px: u16,
    ) -> Option<Offset> {
        self.font
            .mark_to_ligature_offset(ligature, component, mark, size_px)
    }

    fn mark_to_mark_offset(
        &self,
        base_mark: GlyphId,
        mark: GlyphId,
        size_px: u16,
    ) -> Option<Offset> {
        self.font.mark_to_mark_offset(base_mark, mark, size_px)
    }

    fn rasterize(
        &self,
        glyph: GlyphId,
        size_px: u16,
        coverage: &mut [u8],
    ) -> Result<(), FontRasterError> {
        self.rasterize_at(default_instance(size_px), glyph, coverage)
    }

    fn rasterize_with_properties(
        &self,
        properties: FontProperties,
        glyph: GlyphId,
        size_px: u16,
        coverage: &mut [u8],
    ) -> Result<(), FontRasterError> {
        self.rasterize_at(instance_with(properties, size_px), glyph, coverage)
    }

    fn glyph_id_with_properties(
        &self,
        properties: FontProperties,
        character: char,
    ) -> Option<GlyphId> {
        self.font.glyph_id_with_properties(properties, character)
    }

    fn metrics_with_properties(&self, properties: FontProperties, size_px: u16) -> FontMetrics {
        self.font.metrics_with_properties(properties, size_px)
    }

    fn glyph_advance_with_properties(
        &self,
        properties: FontProperties,
        glyph: GlyphId,
        size_px: u16,
    ) -> Option<Pixels> {
        self.font
            .glyph_advance_with_properties(properties, glyph, size_px)
    }

    fn glyph_id_and_advance_with_properties(
        &self,
        properties: FontProperties,
        character: char,
        size_px: u16,
    ) -> Option<(GlyphId, Pixels)> {
        self.font
            .glyph_id_and_advance_with_properties(properties, character, size_px)
    }

    fn glyph_metrics_with_properties(
        &self,
        properties: FontProperties,
        glyph: GlyphId,
        size_px: u16,
    ) -> Option<GlyphMetrics> {
        self.glyph_metrics_at(instance_with(properties, size_px), glyph)
    }

    fn kerning_with_properties(
        &self,
        properties: FontProperties,
        left: GlyphId,
        right: GlyphId,
        size_px: u16,
    ) -> Pixels {
        self.font
            .kerning_with_properties(properties, left, right, size_px)
    }

    fn single_substitution_with_properties(
        &self,
        properties: FontProperties,
        feature: OpenTypeFeature,
        glyph: GlyphId,
    ) -> Option<GlyphId> {
        self.font
            .single_substitution_with_properties(properties, feature, glyph)
    }

    fn ligature_substitution_with_properties(
        &self,
        properties: FontProperties,
        feature: OpenTypeFeature,
        first: GlyphId,
        second: GlyphId,
    ) -> Option<GlyphId> {
        self.font
            .ligature_substitution_with_properties(properties, feature, first, second)
    }

    fn cursive_attachment_with_properties(
        &self,
        properties: FontProperties,
        visual_left: GlyphId,
        visual_right: GlyphId,
        size_px: u16,
        right_to_left: bool,
    ) -> Option<CursiveAttachment> {
        self.font.cursive_attachment_with_properties(
            properties,
            visual_left,
            visual_right,
            size_px,
            right_to_left,
        )
    }

    fn pair_positioning_with_properties(
        &self,
        properties: FontProperties,
        visual_left: GlyphId,
        visual_right: GlyphId,
        size_px: u16,
        right_to_left: bool,
    ) -> PairPositioning {
        self.font.pair_positioning_with_properties(
            properties,
            visual_left,
            visual_right,
            size_px,
            right_to_left,
        )
    }

    fn mark_to_base_offset_with_properties(
        &self,
        properties: FontProperties,
        base: GlyphId,
        mark: GlyphId,
        size_px: u16,
    ) -> Option<Offset> {
        self.font
            .mark_to_base_offset_with_properties(properties, base, mark, size_px)
    }

    fn mark_to_ligature_offset_with_properties(
        &self,
        properties: FontProperties,
        ligature: GlyphId,
        component: u16,
        mark: GlyphId,
        size_px: u16,
    ) -> Option<Offset> {
        self.font
            .mark_to_ligature_offset_with_properties(properties, ligature, component, mark, size_px)
    }

    fn mark_to_mark_offset_with_properties(
        &self,
        properties: FontProperties,
        base_mark: GlyphId,
        mark: GlyphId,
        size_px: u16,
    ) -> Option<Offset> {
        self.font
            .mark_to_mark_offset_with_properties(properties, base_mark, mark, size_px)
    }
}
