//! TrueType glyphs snapped to the pixel grid by skrifa's port of FreeType's
//! autohinter, so stems of one size are drawn the same width in every letter.
//!
//! crosspoint gets its crisp text the same way, from FreeType's autohinter. Only the
//! glyph outlines change: advances, kerning and every other metric come from the
//! unhinted font, so line breaking and shaping are unaffected.

use alloc::{sync::Arc, vec::Vec};

use skrifa::{
    FontRef, MetadataProvider,
    instance::Size,
    outline::{
        DrawSettings, Engine, GlyphStyles, HintingInstance, HintingOptions, OutlineGlyphCollection,
        OutlinePen, SmoothMode, Target,
    },
};

use super::{
    OwnedTtfFont, PreparedTtfFont, TtfFont, TtfFontError, apply_harfrust_properties,
    metrics::glyph_advance_for_font, to_skrifa_glyph,
};
use crate::{
    FillRule, FontData, FontFace, FontMetrics, FontProperties, FontRasterError, FontStyle,
    FontWeight, FontWeightRange, GlyphId, GlyphMetrics, Pixels, PreparedFontSource, VectorPoint,
    float::{ceil_i32, floor_i32},
    px,
    raster::{CoverageGrid, STACK_CELLS, flatten_cubic, flatten_quadratic},
};

/// the reader and the UI each use a few sizes, and a hinting instance costs a pass
/// over the font's style metrics to build
const CACHED_INSTANCES: usize = 4;

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
    cache: spin::Mutex<HintingCache>,
}

impl<'a> HintedTtfFont<'a> {
    pub const fn new(font: TtfFont<'a>) -> Self {
        Self {
            font,
            cache: spin::Mutex::new(HintingCache::new()),
        }
    }

    pub const fn font(&self) -> &TtfFont<'a> {
        &self.font
    }
}

/// An [`OwnedTtfFont`] whose glyph outlines are autohinted by skrifa, so
/// runtime-loaded fonts (e.g. from SD card) render with the same stem snapping
/// as embedded [`HintedTtfFont`] faces.
///
/// Cloning shares both the underlying font bytes and the hinting/shaping cache.
pub struct HintedOwnedTtfFont<D> {
    data: Arc<D>,
    face_index: u32,
    cache: Arc<spin::Mutex<HintingCache>>,
}

impl<D> Clone for HintedOwnedTtfFont<D> {
    fn clone(&self) -> Self {
        Self {
            data: Arc::clone(&self.data),
            face_index: self.face_index,
            cache: Arc::clone(&self.cache),
        }
    }
}

impl<D: AsRef<[u8]> + Send + Sync + 'static> HintedOwnedTtfFont<D> {
    pub fn new(font: OwnedTtfFont<D>) -> Self {
        let (data, face_index) = font.into_parts();
        Self {
            data: Arc::new(data),
            face_index,
            cache: Arc::new(spin::Mutex::new(HintingCache::new())),
        }
    }

    pub fn parse(data: D, face_index: u32) -> Result<Self, TtfFontError> {
        OwnedTtfFont::parse(data, face_index).map(Self::new)
    }

    pub fn data(&self) -> &D {
        self.data.as_ref()
    }

    fn font(&self) -> TtfFont<'_> {
        TtfFont::from_data(FontData::new(self.data.as_ref().as_ref()), self.face_index)
    }
}

struct HintingCache {
    instances: Instances,
    /// the glyph cache asks for a glyph's metrics right before rasterizing it
    last_outline: Option<CachedOutline>,
    /// base `harfrust::Font` holding the shared OpenType layout caches
    harf_base: Option<harfrust::Font>,
    /// most recently used weight-varied `harfrust::Font`
    harf_varied: Option<(FontWeight, harfrust::Font)>,
}

struct Instances {
    /// computed from the whole font on first use, then shared by every instance
    styles: Option<GlyphStyles>,
    /// most recently used last
    cached: Vec<CachedInstance>,
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
    weight: FontWeight,
}

impl HintingCache {
    const fn new() -> Self {
        Self {
            instances: Instances {
                styles: None,
                cached: Vec::new(),
            },
            last_outline: None,
            harf_base: None,
            harf_varied: None,
        }
    }

    fn harfrust_font(
        cache: &spin::Mutex<Self>,
        make_blob: impl FnOnce() -> harfrust::font::Blob,
        face_index: u32,
        properties: FontProperties,
    ) -> Option<harfrust::Font> {
        let mut state = cache.lock();
        let weight = properties.weight();

        if let Some((cached_weight, cached_font)) = &state.harf_varied
            && *cached_weight == weight
        {
            return Some(cached_font.clone());
        }

        let base = match &state.harf_base {
            Some(base) => base,
            None => {
                let font = harfrust::Font::new(make_blob(), face_index)?;
                state.harf_base.insert(font)
            }
        };

        let varied = apply_harfrust_properties(base, properties);
        state.harf_varied = Some((weight, varied.clone()));
        Some(varied)
    }

    fn glyph_metrics(
        cache: &spin::Mutex<Self>,
        font: TtfFont<'_>,
        properties: FontProperties,
        glyph: GlyphId,
        size_px: u16,
    ) -> Option<GlyphMetrics> {
        let font_ref = font.font_ref().ok()?;
        let location = TtfFont::location_for(&font_ref, properties);
        let advance =
            glyph_advance_for_font(&font_ref, (&location).into(), to_skrifa_glyph(glyph), size_px)?;

        let key = InstanceKey {
            size_px,
            weight: properties.weight(),
        };

        Self::with_outline(cache, &font_ref, properties, key, glyph, |outline| {
            outline.metrics(advance)
        })
        .ok()
        .flatten()
    }

    fn rasterize(
        cache: &spin::Mutex<Self>,
        font: TtfFont<'_>,
        properties: FontProperties,
        glyph: GlyphId,
        size_px: u16,
        coverage: &mut [u8],
    ) -> Result<(), FontRasterError> {
        let font_ref = font.font_ref().map_err(|_| FontRasterError::InvalidFont)?;
        let key = InstanceKey {
            size_px,
            weight: properties.weight(),
        };

        Self::with_outline(cache, &font_ref, properties, key, glyph, |outline| {
            outline.rasterize(coverage)
        })?
    }

    fn with_outline<R>(
        cache: &spin::Mutex<Self>,
        font_ref: &FontRef<'_>,
        properties: FontProperties,
        key: InstanceKey,
        glyph: GlyphId,
        use_outline: impl FnOnce(&HintedOutline) -> R,
    ) -> Result<R, FontRasterError> {
        if key.size_px == 0 {
            return Err(FontRasterError::InvalidSize);
        }

        let mut state = cache.lock();

        if let Some(cached) = &state.last_outline
            && cached.key == key
            && cached.glyph == glyph
        {
            return Ok(use_outline(&cached.outline));
        }

        let outlines = font_ref.outline_glyphs();
        let instance = state.instances.get(font_ref, &outlines, properties, key)?;

        let mut recorder = OutlineRecorder::default();

        outlines
            .get(to_skrifa_glyph(glyph))
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

impl Instances {
    fn get(
        &mut self,
        font_ref: &FontRef<'_>,
        outlines: &OutlineGlyphCollection<'_>,
        properties: FontProperties,
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

            let location = TtfFont::location_for(font_ref, properties);
            let instance = HintingInstance::new(
                outlines,
                Size::new(f32::from(key.size_px)),
                &location,
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

    /// bitmap bounds in pixels, y down, like `glyph_metrics_for_font`
    fn pixel_bounds(&self) -> Option<(i32, i32, i32, i32)> {
        let (left, bottom, right, top) = self.bounds?;

        let left = floor_i32(left);
        let top = floor_i32(-top);
        let width = ceil_i32(right).saturating_sub(left).max(0);
        let height = ceil_i32(-bottom).saturating_sub(top).max(0);

        Some((left, top, width, height))
    }

    /// flattens the outline into directed Y-down segments once, so rasterizing it
    /// runs a single analytical pass with no curve re-evaluation
    fn finish(self) -> HintedOutline {
        let Some(bounds) = self.pixel_bounds() else {
            return HintedOutline {
                bounds: None,
                segments: Vec::new(),
            };
        };

        let mut segments = Vec::new();
        let mut current: Option<VectorPoint> = None;
        let mut contour_start: Option<VectorPoint> = None;

        let close_contour = |current: &mut Option<VectorPoint>,
                             contour_start: &mut Option<VectorPoint>,
                             segments: &mut Vec<(VectorPoint, VectorPoint)>| {
            if let (Some(from), Some(start)) = (current.take(), contour_start.take())
                && from.y != start.y
            {
                segments.push((from, start));
            }
        };

        for command in &self.commands {
            match *command {
                PathCommand::MoveTo(x, y) => {
                    close_contour(&mut current, &mut contour_start, &mut segments);
                    let point = VectorPoint::new(x, -y);
                    current = Some(point);
                    contour_start = Some(point);
                }
                PathCommand::LineTo(x, y) => {
                    let to = VectorPoint::new(x, -y);
                    if let Some(from) = current
                        && from.y != to.y
                    {
                        segments.push((from, to));
                    }
                    current = Some(to);
                }
                PathCommand::QuadTo(x1, y1, x, y) => {
                    let end = VectorPoint::new(x, -y);
                    let Some(start) = current else {
                        current = Some(end);
                        continue;
                    };
                    let control = VectorPoint::new(x1, -y1);
                    flatten_quadratic(start, control, end, |from, to| {
                        if from.y != to.y {
                            segments.push((from, to));
                        }
                    });
                    current = Some(end);
                }
                PathCommand::CurveTo(x1, y1, x2, y2, x, y) => {
                    let end = VectorPoint::new(x, -y);
                    let Some(start) = current else {
                        current = Some(end);
                        continue;
                    };
                    let c1 = VectorPoint::new(x1, -y1);
                    let c2 = VectorPoint::new(x2, -y2);
                    flatten_cubic(start, c1, c2, end, |from, to| {
                        if from.y != to.y {
                            segments.push((from, to));
                        }
                    });
                    current = Some(end);
                }
                PathCommand::Close => {
                    close_contour(&mut current, &mut contour_start, &mut segments);
                }
            }
        }

        close_contour(&mut current, &mut contour_start, &mut segments);

        HintedOutline {
            bounds: Some(bounds),
            segments,
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

/// a hinted outline flattened into Y-down pixel space
struct HintedOutline {
    /// left, top, width, height in pixels, y down
    bounds: Option<(i32, i32, i32, i32)>,
    segments: Vec<(VectorPoint, VectorPoint)>,
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

        if required == 0 {
            return Ok(());
        }

        let cells_len = width
            .checked_add(1)
            .and_then(|stride| stride.checked_mul(height))
            .ok_or(FontRasterError::InvalidGlyph)?;

        if cells_len <= STACK_CELLS {
            let mut cells = [0.0f32; STACK_CELLS];
            let mut grid = CoverageGrid::new(&mut cells, width, height, left, top);
            for &(from, to) in &self.segments {
                grid.add_segment(from, to);
            }
            grid.write_coverage(FillRule::NonZero, coverage);
        } else {
            let mut cells = alloc::vec![0.0f32; cells_len];
            let mut grid = CoverageGrid::new(&mut cells, width, height, left, top);
            for &(from, to) in &self.segments {
                grid.add_segment(from, to);
            }
            grid.write_coverage(FillRule::NonZero, coverage);
        }

        Ok(())
    }
}

impl FontFace for HintedTtfFont<'static> {
    fn weight_range(&self) -> FontWeightRange {
        self.font.weight_range()
    }

    fn style(&self) -> FontStyle {
        self.font.style()
    }

    fn prepare(&self, properties: FontProperties) -> PreparedFontSource<'_> {
        match self.font.font_ref() {
            Ok(font) => {
                let location = TtfFont::location_for(&font, properties);
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
        HintingCache::harfrust_font(
            &self.cache,
            || harfrust::font::Blob::Static(self.font.data().bytes()),
            self.font.face_index(),
            properties,
        )
    }

    fn glyph_id(&self, character: char) -> Option<GlyphId> {
        self.font.glyph_id(character)
    }

    fn metrics(&self, properties: FontProperties, size_px: u16) -> FontMetrics {
        self.font.metrics(properties, size_px)
    }

    fn glyph_advance(
        &self,
        properties: FontProperties,
        glyph: GlyphId,
        size_px: u16,
    ) -> Option<Pixels> {
        self.font.glyph_advance(properties, glyph, size_px)
    }

    fn glyph_id_and_advance(
        &self,
        properties: FontProperties,
        character: char,
        size_px: u16,
    ) -> Option<(GlyphId, Pixels)> {
        self.font
            .glyph_id_and_advance(properties, character, size_px)
    }

    fn glyph_metrics(
        &self,
        properties: FontProperties,
        glyph: GlyphId,
        size_px: u16,
    ) -> Option<GlyphMetrics> {
        HintingCache::glyph_metrics(&self.cache, self.font, properties, glyph, size_px)
    }

    fn rasterize(
        &self,
        properties: FontProperties,
        glyph: GlyphId,
        size_px: u16,
        coverage: &mut [u8],
    ) -> Result<(), FontRasterError> {
        HintingCache::rasterize(&self.cache, self.font, properties, glyph, size_px, coverage)
    }
}

impl<D: AsRef<[u8]> + Send + Sync + 'static> FontFace for HintedOwnedTtfFont<D> {
    fn weight_range(&self) -> FontWeightRange {
        self.font().weight_range()
    }

    fn style(&self) -> FontStyle {
        self.font().style()
    }

    fn prepare(&self, properties: FontProperties) -> PreparedFontSource<'_> {
        match self.font().font_ref() {
            Ok(font) => {
                let location = TtfFont::location_for(&font, properties);
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
        HintingCache::harfrust_font(
            &self.cache,
            || {
                let shared: Arc<dyn AsRef<[u8]> + Send + Sync> = Arc::<D>::clone(&self.data);
                harfrust::font::Blob::Shared(shared)
            },
            self.face_index,
            properties,
        )
    }

    fn glyph_id(&self, character: char) -> Option<GlyphId> {
        self.font().glyph_id(character)
    }

    fn metrics(&self, properties: FontProperties, size_px: u16) -> FontMetrics {
        self.font().metrics(properties, size_px)
    }

    fn glyph_advance(
        &self,
        properties: FontProperties,
        glyph: GlyphId,
        size_px: u16,
    ) -> Option<Pixels> {
        self.font().glyph_advance(properties, glyph, size_px)
    }

    fn glyph_id_and_advance(
        &self,
        properties: FontProperties,
        character: char,
        size_px: u16,
    ) -> Option<(GlyphId, Pixels)> {
        self.font()
            .glyph_id_and_advance(properties, character, size_px)
    }

    fn glyph_metrics(
        &self,
        properties: FontProperties,
        glyph: GlyphId,
        size_px: u16,
    ) -> Option<GlyphMetrics> {
        HintingCache::glyph_metrics(&self.cache, self.font(), properties, glyph, size_px)
    }

    fn rasterize(
        &self,
        properties: FontProperties,
        glyph: GlyphId,
        size_px: u16,
        coverage: &mut [u8],
    ) -> Result<(), FontRasterError> {
        HintingCache::rasterize(&self.cache, self.font(), properties, glyph, size_px, coverage)
    }
}
