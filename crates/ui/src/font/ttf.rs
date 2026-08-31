use ttf_parser::{
    Face, GlyphId as TtfGlyphId, OutlineBuilder, Tag,
    gpos::{Anchor, PairAdjustment, PositioningSubtable},
    gsub::{SingleSubstitution, SubstitutionSubtable},
};

use crate::{
    CursiveAttachment, FontData, FontFace, FontMetrics, FontRasterError, GlyphId, GlyphMetrics,
    Offset, OpenTypeFeature, Pixels, px,
};

const SUPERSAMPLE_X: usize = 4;
const SUPERSAMPLE_Y: usize = 4;
const COVERAGE_SAMPLES: u16 = (SUPERSAMPLE_X * SUPERSAMPLE_Y) as u16;

/// this is intentionally bounded
///
/// we don't allocate an edge/intersection vector while rasterizing. If a particular
/// outline exceeds the capacity, rasterization fails explecitly instead of silently
/// consuming heap memory.
///
/// Arabic/Latin glyphs are normally below this. If real fonts later demonstrate that
/// the value is too small, we can measure and tune it
const MAX_SCANLINE_INTERSECTIONS: usize = 96;

const MAX_CURVE_STEPS: usize = 32;
const CURVE_PIXELS_PER_STEP: f32 = 2.0;

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
    pub fn parse(data: FontData<'a>, face_index: u32) -> Result<Self, TtfFontError> {
        Face::parse(data.bytes(), face_index).map_err(|_| TtfFontError::InvalidFont)?;

        Ok(Self { data, face_index })
    }

    pub const fn data(self) -> FontData<'a> {
        self.data
    }

    pub const fn face_index(self) -> u32 {
        self.face_index
    }

    fn face(&self) -> Result<Face<'a>, TtfFontError> {
        Face::parse(self.data.bytes(), self.face_index).map_err(|_| TtfFontError::InvalidFont)
    }
}

impl FontFace for TtfFont<'_> {
    fn glyph_id(&self, character: char) -> Option<GlyphId> {
        let face = self.face().ok()?;
        let glyph = face.glyph_index(character)?;

        Some(GlyphId::new(glyph.0))
    }

    fn metrics(&self, size_px: u16) -> FontMetrics {
        let Ok(face) = self.face() else {
            return FontMetrics::default();
        };

        let Some(scale) = font_scale(&face, size_px) else {
            return FontMetrics::default();
        };

        FontMetrics::new(
            positive_scaled_units(i32::from(face.ascender()), scale),
            positive_scaled_units(i32::from(face.descender()).saturating_neg(), scale),
            positive_scaled_units(i32::from(face.line_gap()), scale),
        )
    }

    fn glyph_advance(&self, glyph: GlyphId, size_px: u16) -> Option<Pixels> {
        let face = self.face().ok()?;

        glyph_advance_for_face(&face, to_ttf_glyph(glyph), size_px)
    }

    fn glyph_metrics(&self, glyph: GlyphId, size_px: u16) -> Option<GlyphMetrics> {
        let face = self.face().ok()?;

        glyph_metrics_for_face(&face, to_ttf_glyph(glyph), size_px)
    }

    fn kerning(&self, left: GlyphId, right: GlyphId, size_px: u16) -> Pixels {
        let Ok(face) = self.face() else { return px(0) };

        let left = to_ttf_glyph(left);
        let right = to_ttf_glyph(right);

        if let Some(adjustment) = gpos_kerning_for_face(&face, left, right, size_px) {
            return adjustment;
        }

        legacy_kerning_for_face(&face, left, right, size_px).unwrap_or(px(0))
    }

    fn single_substitution(&self, feature: OpenTypeFeature, glyph: GlyphId) -> Option<GlyphId> {
        let face = self.face().ok()?;

        let substituted = gsub_single_substitution_for_face(&face, feature, to_ttf_glyph(glyph))?;

        Some(GlyphId::new(substituted.0))
    }

    fn ligature_substitution(
        &self,
        feature: OpenTypeFeature,
        first: GlyphId,
        second: GlyphId,
    ) -> Option<GlyphId> {
        let face = self.face().ok()?;

        let substituted =
            gsub_pair_ligature_for_face(&face, feature, to_ttf_glyph(first), to_ttf_glyph(second))?;

        Some(GlyphId::new(substituted.0))
    }

    fn cursive_attachment(
        &self,
        visual_left: GlyphId,
        visual_right: GlyphId,
        size_px: u16,
        right_to_left: bool,
    ) -> Option<CursiveAttachment> {
        let face = self.face().ok()?;

        gpos_cursive_attachment_for_face(
            &face,
            to_ttf_glyph(visual_left),
            to_ttf_glyph(visual_right),
            size_px,
            right_to_left,
        )
    }

    fn mark_to_base_offset(&self, base: GlyphId, mark: GlyphId, size_px: u16) -> Option<Offset> {
        let face = self.face().ok()?;

        mark_to_base_offset_for_face(&face, to_ttf_glyph(base), to_ttf_glyph(mark), size_px)
    }

    fn mark_to_ligature_offset(
        &self,
        ligature: GlyphId,
        component: u16,
        mark: GlyphId,
        size_px: u16,
    ) -> Option<Offset> {
        let face = self.face().ok()?;

        mark_to_ligature_offset_for_face(
            &face,
            to_ttf_glyph(ligature),
            component,
            to_ttf_glyph(mark),
            size_px,
        )
    }

    fn mark_to_mark_offset(
        &self,
        base_mark: GlyphId,
        mark: GlyphId,
        size_px: u16,
    ) -> Option<Offset> {
        let face = self.face().ok()?;

        mark_to_mark_offset_for_face(&face, to_ttf_glyph(base_mark), to_ttf_glyph(mark), size_px)
    }

    fn rasterize(
        &self,
        glyph: GlyphId,
        size_px: u16,
        coverage: &mut [u8],
    ) -> Result<(), FontRasterError> {
        if size_px == 0 {
            return Err(FontRasterError::InvalidSize);
        }

        let face = self.face().map_err(|_| FontRasterError::InvalidFont)?;
        let glyph = to_ttf_glyph(glyph);
        let metrics =
            glyph_metrics_for_face(&face, glyph, size_px).ok_or(FontRasterError::InvalidGlyph)?;
        let required = metrics
            .coverage_bytes()
            .ok_or(FontRasterError::InvalidGlyph)?;

        if coverage.len() < required {
            return Err(FontRasterError::BufferTooSmall);
        }

        coverage[..required].fill(0);

        if required == 0 {
            // whitespace and otehr zero-outline glyphs are valid
            return Ok(());
        }

        let Some(scale) = font_scale(&face, size_px) else {
            return Err(FontRasterError::InvalidSize);
        };

        let width = usize::from(metrics.width);
        let height = usize::from(metrics.height);
        let left = metrics.bearing_x.get();
        let top = metrics.bearing_y.get();

        for row in 0..height {
            for sub_y in 0..SUPERSAMPLE_Y {
                let sample_y = row as f32 + (sub_y as f32 + 0.5) / SUPERSAMPLE_Y as f32;
                let mut scanline = ScanlineBuilder::new(scale, left, top, sample_y);

                if face.outline_glyph(glyph, &mut scanline).is_none() {
                    return Err(FontRasterError::Unsupported);
                }
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
}

fn to_ttf_glyph(glyph: GlyphId) -> TtfGlyphId {
    TtfGlyphId(glyph.value())
}

fn gsub_single_substitution_for_face(
    face: &Face<'_>,
    feature: OpenTypeFeature,
    glyph: TtfGlyphId,
) -> Option<TtfGlyphId> {
    let gsub = face.tables().gsub?;
    let feature_bytes = feature.tag();
    let feature_tag = Tag::from_bytes(&feature_bytes);

    for feature_record in gsub.features {
        if feature_record.tag != feature_tag {
            continue;
        }

        let mut current = glyph;
        let mut changed = false;

        // lookups inside a feature are applied in order.
        for lookup_index in feature_record.lookup_indices {
            let Some(lookup) = gsub.lookups.get(lookup_index) else {
                continue;
            };

            // subtables within one lookup are alternatives.
            // apply the first one that handles the current glyph.
            for subtable in lookup.subtables.into_iter::<SubstitutionSubtable>() {
                let Some(next) = apply_single_output_substitution(subtable, current) else {
                    continue;
                };

                current = next;
                changed = true;

                // subtables inside one lookup are alternatives, not a pipeline.
                break;
            }
        }

        if changed {
            return Some(current);
        }
    }

    None
}

fn apply_single_substitution(
    substitution: SingleSubstitution<'_>,
    glyph: TtfGlyphId,
) -> Option<TtfGlyphId> {
    match substitution {
        SingleSubstitution::Format1 { coverage, delta } => {
            coverage.get(glyph)?;

            // OpenType SingleSubst format 1 adds the signed delta modulo 65536 to
            // the original glyph ID.
            Some(TtfGlyphId(glyph.0.wrapping_add(delta as u16)))
        }
        SingleSubstitution::Format2 {
            coverage,
            substitutes,
        } => {
            let index = coverage.get(glyph)?;

            substitutes.get(index)
        }
    }
}

fn apply_single_output_substitution(
    substitution: SubstitutionSubtable<'_>,
    glyph: TtfGlyphId,
) -> Option<TtfGlyphId> {
    match substitution {
        SubstitutionSubtable::Single(substitution) => {
            apply_single_substitution(substitution, glyph)
        }

        SubstitutionSubtable::Multiple(substitution) => {
            let index = substitution.coverage.get(glyph)?;

            let sequence = substitution.sequences.get(index)?;

            // MultipleSubstitution is only compatible with this bounded one-glyph API
            // when it produces exactly one glyph.
            // zero outputs means deletion.
            // more than one output requires a real glyph buffer rewrite and is
            // deliberately unsupported here.
            if sequence.substitutes.len() != 1 {
                return None;
            }

            sequence.substitutes.get(0)
        }
        _ => None,
    }
}

fn gsub_pair_ligature_for_face(
    face: &Face<'_>,
    feature: OpenTypeFeature,
    first: TtfGlyphId,
    second: TtfGlyphId,
) -> Option<TtfGlyphId> {
    let gsub = face.tables().gsub?;
    let feature_bytes = feature.tag();
    let feature_tag = Tag::from_bytes(&feature_bytes);

    for feature_record in gsub.features {
        if feature_record.tag != feature_tag {
            continue;
        }

        for lookup_index in feature_record.lookup_indices {
            let Some(lookup) = gsub.lookups.get(lookup_index) else {
                continue;
            };

            for subtable in lookup.subtables.into_iter::<SubstitutionSubtable>() {
                let SubstitutionSubtable::Ligature(substitution) = subtable else {
                    continue;
                };
                let Some(first_index) = substitution.coverage.get(first) else {
                    continue;
                };
                let Some(set) = substitution.ligature_sets.get(first_index) else {
                    continue;
                };

                let mut index = 0u16;

                while index < set.len() {
                    let Some(ligature) = set.get(index) else {
                        index = index.saturating_add(1);

                        continue;
                    };

                    // this bounded API deliberately handles exactly a two-glyph ligature.
                    if ligature.components.len() == 1 && ligature.components.get(0) == Some(second)
                    {
                        return Some(ligature.glyph);
                    }

                    index = index.saturating_add(1);
                }
            }
        }
    }

    None
}

fn gpos_cursive_attachment_for_face(
    face: &Face<'_>,
    visual_left: TtfGlyphId,
    visual_right: TtfGlyphId,
    size_px: u16,
    right_to_left: bool,
) -> Option<CursiveAttachment> {
    let scale = font_scale(face, size_px)?;
    let gpos = face.tables().gpos?;
    let curs_tag = Tag::from_bytes(b"curs");

    for feature in gpos.features {
        if feature.tag != curs_tag {
            continue;
        }

        for lookup_index in feature.lookup_indices {
            let Some(lookup) = gpos.lookups.get(lookup_index) else {
                continue;
            };

            // supporting the opposite baseline-root direction requires maintaining
            // and reversing an attachment graph.
            // for our bounded shaper, only accept the normal case where lookup direction
            // agrees with the bidi run direction.
            if lookup.flags.right_to_left() != right_to_left {
                continue;
            }

            for subtable in lookup.subtables.into_iter::<PositioningSubtable>() {
                let PositioningSubtable::Cursive(adjustment) = subtable else {
                    continue;
                };
                let Some(left_index) = adjustment.coverage.get(visual_left) else {
                    continue;
                };
                let Some(right_index) = adjustment.coverage.get(visual_right) else {
                    continue;
                };
                let origin_delta = if right_to_left {
                    // visual order:
                    //     logical second | logical first
                    //        left              right
                    // the exit anchor of the logical first glyph attaches to the entry
                    // anchor of logical second.
                    let Some(left_entry) = adjustment.sets.entry(left_index) else {
                        continue;
                    };
                    let Some(right_exit) = adjustment.sets.exit(right_index) else {
                        continue;
                    };

                    anchor_attachment_offset(left_entry, right_exit, scale)
                } else {
                    // LTR visual order is also logical order:
                    //     left.exit == right.entry
                    let Some(left_exit) = adjustment.sets.exit(left_index) else {
                        continue;
                    };
                    let Some(right_entry) = adjustment.sets.entry(right_index) else {
                        continue;
                    };

                    anchor_attachment_offset(left_exit, right_entry, scale)
                };

                return Some(CursiveAttachment::new(origin_delta));
            }
        }
    }

    None
}

fn gpos_kerning_for_face(
    face: &Face<'_>,
    left: TtfGlyphId,
    right: TtfGlyphId,
    size_px: u16,
) -> Option<Pixels> {
    let scale = font_scale(face, size_px)?;
    let gpos = face.tables().gpos?;
    let kern_tag = Tag::from_bytes(b"kern");

    for feature in gpos.features {
        if feature.tag != kern_tag {
            continue;
        }

        for lookup_index in feature.lookup_indices {
            let Some(lookup) = gpos.lookups.get(lookup_index) else {
                continue;
            };

            for subtable in lookup.subtables.into_iter::<PositioningSubtable>() {
                let PositioningSubtable::Pair(adjustment) = subtable else {
                    continue;
                };

                let pair = match adjustment {
                    PairAdjustment::Format1 { coverage, sets } => {
                        let Some(left_index) = coverage.get(left) else {
                            continue;
                        };
                        let Some(set) = sets.get(left_index) else {
                            continue;
                        };

                        set.get(right)
                    }

                    PairAdjustment::Format2 {
                        coverage,
                        classes,
                        matrix,
                    } => {
                        if coverage.get(left).is_none() {
                            continue;
                        }

                        let left_class = classes.0.get(left);
                        let right_class = classes.1.get(right);

                        matrix.get((left_class, right_class))
                    }
                };

                let Some((left_value, _right_value)) = pair else {
                    continue;
                };

                // InkPaper's current FontFace::kerning contract is a scalar horizontal pair
                // advance adjustment.
                // OpenType kerning conventionally expresses this in the first glyph's
                // xAdvance value.
                // placement fields and the second glyph's advance are deliberately not
                // folded into this scalar because doing so would change their semantics.
                return Some(px(round_to_i32(f32::from(left_value.x_advance) * scale)));
            }
        }
    }

    None
}

fn legacy_kerning_for_face(
    face: &Face<'_>,
    left: TtfGlyphId,
    right: TtfGlyphId,
    size_px: u16,
) -> Option<Pixels> {
    let scale = font_scale(face, size_px)?;
    let kern = face.tables().kern?;

    for subtable in kern.subtables {
        if !subtable.horizontal
            || subtable.variable
            || subtable.has_cross_stream
            || subtable.has_state_machine
        {
            continue;
        }

        if let Some(value) = subtable.glyphs_kerning(left, right) {
            return Some(px(round_to_i32(f32::from(value) * scale)));
        }
    }

    None
}

fn mark_to_base_offset_for_face(
    face: &Face<'_>,
    base: TtfGlyphId,
    mark: TtfGlyphId,
    size_px: u16,
) -> Option<Offset> {
    let scale = font_scale(face, size_px)?;
    let gpos = face.tables().gpos?;
    let mark_tag = Tag::from_bytes(b"mark");

    for feature in gpos.features {
        if feature.tag != mark_tag {
            continue;
        }

        for lookup_index in feature.lookup_indices {
            let Some(lookup) = gpos.lookups.get(lookup_index) else {
                continue;
            };

            for subtable in lookup.subtables.into_iter::<PositioningSubtable>() {
                let PositioningSubtable::MarkToBase(adjustment) = subtable else {
                    continue;
                };
                let Some(mark_index) = adjustment.mark_coverage.get(mark) else {
                    continue;
                };
                let Some(base_index) = adjustment.base_coverage.get(base) else {
                    continue;
                };
                let Some((class, mark_anchor)) = adjustment.marks.get(mark_index) else {
                    continue;
                };
                let Some(base_anchor) = adjustment.anchors.get(base_index, class) else {
                    continue;
                };

                return Some(anchor_attachment_offset(base_anchor, mark_anchor, scale));
            }
        }
    }

    None
}

fn mark_to_ligature_offset_for_face(
    face: &Face<'_>,
    ligature: TtfGlyphId,
    component: u16,
    mark: TtfGlyphId,
    size_px: u16,
) -> Option<Offset> {
    let scale = font_scale(face, size_px)?;
    let gpos = face.tables().gpos?;
    let mark_tag = Tag::from_bytes(b"mark");

    for feature in gpos.features {
        if feature.tag != mark_tag {
            continue;
        }

        for lookup_index in feature.lookup_indices {
            let Some(lookup) = gpos.lookups.get(lookup_index) else {
                continue;
            };

            for subtable in lookup.subtables.into_iter::<PositioningSubtable>() {
                let PositioningSubtable::MarkToLigature(adjustment) = subtable else {
                    continue;
                };
                let Some(mark_index) = adjustment.mark_coverage.get(mark) else {
                    continue;
                };
                let Some(ligature_index) = adjustment.ligature_coverage.get(ligature) else {
                    continue;
                };
                let Some((class, mark_anchor)) = adjustment.marks.get(mark_index) else {
                    continue;
                };
                let Some(matrix) = adjustment.ligature_array.get(ligature_index) else {
                    continue;
                };
                let Some(ligature_anchor) = matrix.get(component, class) else {
                    continue;
                };

                return Some(anchor_attachment_offset(
                    ligature_anchor,
                    mark_anchor,
                    scale,
                ));
            }
        }
    }

    None
}

fn mark_to_mark_offset_for_face(
    face: &Face<'_>,
    base_mark: TtfGlyphId,
    mark: TtfGlyphId,
    size_px: u16,
) -> Option<Offset> {
    let scale = font_scale(face, size_px)?;
    let gpos = face.tables().gpos?;
    let mark_tag = Tag::from_bytes(b"mkmk");

    for feature in gpos.features {
        if feature.tag != mark_tag {
            continue;
        }

        for lookup_index in feature.lookup_indices {
            let Some(lookup) = gpos.lookups.get(lookup_index) else {
                continue;
            };

            for subtable in lookup.subtables.into_iter::<PositioningSubtable>() {
                let PositioningSubtable::MarkToMark(adjustment) = subtable else {
                    continue;
                };
                // mark1 is the child mark being positioned.
                let Some(mark_index) = adjustment.mark1_coverage.get(mark) else {
                    continue;
                };
                // mark2 is the already-positioned
                // attachment mark.
                let Some(base_index) = adjustment.mark2_coverage.get(base_mark) else {
                    continue;
                };
                let Some((class, mark_anchor)) = adjustment.marks.get(mark_index) else {
                    continue;
                };
                let Some(base_anchor) = adjustment.mark2_matrix.get(base_index, class) else {
                    continue;
                };

                return Some(anchor_attachment_offset(base_anchor, mark_anchor, scale));
            }
        }
    }

    None
}

fn anchor_attachment_offset(parent: Anchor<'_>, child: Anchor<'_>, scale: f32) -> Offset {
    // OpenType attachment means:
    //     child_origin + child_anchor
    //         ==
    //     parent_origin + parent_anchor
    // therefore:
    //     child_origin - parent_origin
    //         =
    //     parent_anchor - child_anchor
    // OpenType's Y axis grows upward while InkPaper's framebuffer Y grows downward,
    // hence the reversed subtraction on Y.
    let x_units = i32::from(parent.x) - i32::from(child.x);
    let y_units = i32::from(child.y) - i32::from(parent.y);

    Offset::new(
        px(round_to_i32(x_units as f32 * scale)),
        px(round_to_i32(y_units as f32 * scale)),
    )
}

fn font_scale(face: &Face<'_>, size_px: u16) -> Option<f32> {
    if size_px == 0 {
        return None;
    }

    let units_per_em = face.units_per_em();
    if units_per_em == 0 {
        return None;
    }

    Some(f32::from(size_px) / f32::from(units_per_em))
}

fn positive_scaled_units(units: i32, scale: f32) -> Pixels {
    if units <= 0 {
        return px(0);
    }

    px(ceil_to_i32(units as f32 * scale))
}

fn glyph_advance_for_face(face: &Face<'_>, glyph: TtfGlyphId, size_px: u16) -> Option<Pixels> {
    let scale = font_scale(face, size_px)?;
    let advance = face.glyph_hor_advance(glyph)?;

    Some(px(round_to_i32(f32::from(advance) * scale)))
}

fn glyph_metrics_for_face(
    face: &Face<'_>,
    glyph: TtfGlyphId,
    size_px: u16,
) -> Option<GlyphMetrics> {
    let scale = font_scale(face, size_px)?;
    let advance = glyph_advance_for_face(face, glyph, size_px)?;

    let Some(bounds) = face.glyph_bounding_box(glyph) else {
        return Some(GlyphMetrics::new(0, 0, px(0), px(0), advance));
    };

    // font coordinates are Y-up. Our framebuffer coordinates are Y-down
    let left = floor_to_i32(f32::from(bounds.x_min) * scale);
    let right = ceil_to_i32(f32::from(bounds.x_max) * scale);
    let top = floor_to_i32(-f32::from(bounds.y_max) * scale);
    let bottom = ceil_to_i32(-f32::from(bounds.y_min) * scale);
    let width = right.saturating_sub(left).max(0);
    let height = bottom.saturating_sub(top).max(0);

    Some(GlyphMetrics::new(
        u16::try_from(width).ok()?,
        u16::try_from(height).ok()?,
        px(left),
        px(top),
        advance,
    ))
}

#[derive(Clone, Copy)]
struct RasterPoint {
    x: f32,
    y: f32,
}

impl RasterPoint {
    const ZERO: Self = Self { x: 0.0, y: 0.0 };
}

#[derive(Clone, Copy)]
struct Intersection {
    x: f32,
    winding: i8,
}

impl Intersection {
    const EMPTY: Self = Self { x: 0.0, winding: 0 };
}

struct ScanlineBuilder {
    scale: f32,
    left: f32,
    top: f32,
    sample_y: f32,

    current: Option<RasterPoint>,
    contour_start: Option<RasterPoint>,
    intersections: [Intersection; MAX_SCANLINE_INTERSECTIONS],

    len: usize,
    overflowed: bool,
}

impl ScanlineBuilder {
    fn new(scale: f32, left: i32, top: i32, sample_y: f32) -> Self {
        Self {
            scale,
            left: left as f32,
            top: top as f32,
            sample_y,
            current: None,
            contour_start: None,
            intersections: [Intersection::EMPTY; MAX_SCANLINE_INTERSECTIONS],
            len: 0,
            overflowed: false,
        }
    }

    fn transform(&self, x: f32, y: f32) -> RasterPoint {
        RasterPoint {
            x: x * self.scale - self.left,
            y: -y * self.scale - self.top,
        }
    }

    fn push_intersection(&mut self, intersection: Intersection) {
        if self.len >= MAX_SCANLINE_INTERSECTIONS {
            self.overflowed = true;
            return;
        }

        self.intersections[self.len] = intersection;
        self.len += 1;
    }

    fn add_segment(&mut self, from: RasterPoint, to: RasterPoint) {
        let winding = if from.y <= self.sample_y && self.sample_y < to.y {
            1
        } else if to.y <= self.sample_y && self.sample_y < from.y {
            -1
        } else {
            return;
        };

        let denominator = to.y - from.y;
        if denominator == 0.0 {
            return;
        }

        let t = (self.sample_y - from.y) / denominator;
        let x = from.x + (to.x - from.x) * t;

        self.push_intersection(Intersection { x, winding });
    }

    fn sort_intersections(&mut self) {
        // intersection counts are intentionally tiny and bounded, so an allocation-free
        // insertion sort is appropriate here.
        let mut index = 1usize;

        while index < self.len {
            let value = self.intersections[index];
            let mut cursor = index;

            while cursor > 0 && self.intersections[cursor - 1].x > value.x {
                self.intersections[cursor] = self.intersections[cursor - 1];
                cursor -= 1;
            }

            self.intersections[cursor] = value;
            index += 1;
        }
    }

    fn intersections(&self) -> &[Intersection] {
        &self.intersections[..self.len]
    }

    const fn overflowed(&self) -> bool {
        self.overflowed
    }
}

impl OutlineBuilder for ScanlineBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        let point = self.transform(x, y);

        self.current = Some(point);
        self.contour_start = Some(point);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let to = self.transform(x, y);

        if let Some(from) = self.current {
            self.add_segment(from, to);
        }

        self.current = Some(to);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let end = self.transform(x, y);

        let Some(start) = self.current else {
            self.current = Some(end);

            return;
        };

        let control = self.transform(x1, y1);
        let steps = curve_steps(&[start, control, end]);
        let mut previous = start;

        for step in 1..=steps {
            let t = step as f32 / steps as f32;
            let inverse = 1.0 - t;
            let point = RasterPoint {
                x: inverse * inverse * start.x + 2.0 * inverse * t * control.x + t * t * end.x,
                y: inverse * inverse * start.y + 2.0 * inverse * t * control.y + t * t * end.y,
            };

            self.add_segment(previous, point);

            previous = point;
        }

        self.current = Some(end);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let end = self.transform(x, y);

        let Some(start) = self.current else {
            self.current = Some(end);

            return;
        };

        let control_1 = self.transform(x1, y1);
        let control_2 = self.transform(x2, y2);
        let steps = curve_steps(&[start, control_1, control_2, end]);
        let mut previous = start;

        for step in 1..=steps {
            let t = step as f32 / steps as f32;
            let inverse = 1.0 - t;
            let inverse_2 = inverse * inverse;
            let t_2 = t * t;

            let point = RasterPoint {
                x: inverse_2 * inverse * start.x
                    + 3.0 * inverse_2 * t * control_1.x
                    + 3.0 * inverse * t_2 * control_2.x
                    + t_2 * t * end.x,
                y: inverse_2 * inverse * start.y
                    + 3.0 * inverse_2 * t * control_1.y
                    + 3.0 * inverse * t_2 * control_2.y
                    + t_2 * t * end.y,
            };

            self.add_segment(previous, point);

            previous = point;
        }

        self.current = Some(end);
    }

    fn close(&mut self) {
        if let (Some(current), Some(start)) = (self.current, self.contour_start) {
            self.add_segment(current, start);
        }

        self.current = None;
        self.contour_start = None;
    }
}

fn curve_steps(points: &[RasterPoint]) -> usize {
    if points.len() < 2 {
        return 1;
    }

    let mut length = 0.0f32;

    for pair in points.windows(2) {
        length += approximate_distance(pair[0], pair[1]);
    }

    let estimate = length / CURVE_PIXELS_PER_STEP;
    let truncated = estimate as usize;

    let mut steps = if (truncated as f32) < estimate {
        truncated.saturating_add(1)
    } else {
        truncated
    };

    if steps == 0 {
        steps = 1;
    }

    steps.min(MAX_CURVE_STEPS)
}

fn approximate_distance(from: RasterPoint, to: RasterPoint) -> f32 {
    abs_f32(to.x - from.x) + abs_f32(to.y - from.y)
}

fn accumulate_scanline(
    width: usize,
    row: usize,
    coverage: &mut [u8],
    intersections: &[Intersection],
) {
    if intersections.is_empty() {
        return;
    }

    let mut index = 0usize;
    let mut winding = 0i16;
    let mut previous_x = intersections[0].x;

    while index < intersections.len() {
        let x = intersections[index].x;
        if winding != 0 {
            accumulate_interval(width, row, coverage, previous_x, x);
        }

        let mut delta = 0i16;

        while index < intersections.len() && intersections[index].x == x {
            delta += i16::from(intersections[index].winding);
            index += 1;
        }

        winding += delta;
        previous_x = x;
    }
}

fn accumulate_interval(width: usize, row: usize, coverage: &mut [u8], start: f32, end: f32) {
    if width == 0 || end <= start {
        return;
    }

    let start = if start < 0.0 { 0.0 } else { start };
    let max_x = width as f32;
    let end = if end > max_x { max_x } else { end };
    if end <= start {
        return;
    }

    let first_pixel = floor_to_i32(start).max(0);
    let last_pixel = ceil_to_i32(end).min(i32::try_from(width).unwrap_or(i32::MAX));
    let Ok(first_pixel) = usize::try_from(first_pixel) else {
        return;
    };

    let Ok(last_pixel) = usize::try_from(last_pixel) else {
        return;
    };

    for x in first_pixel..last_pixel {
        let Some(pixel) = row
            .checked_mul(width)
            .and_then(|row_start| row_start.checked_add(x))
            .and_then(|index| coverage.get_mut(index))
        else {
            continue;
        };

        for sub_x in 0..SUPERSAMPLE_X {
            let sample_x = x as f32 + (sub_x as f32 + 0.5) / SUPERSAMPLE_X as f32;
            if sample_x >= start && sample_x < end {
                *pixel = pixel.saturating_add(1);
            }
        }
    }
}

fn normalize_coverage(coverage: &mut [u8]) {
    for value in coverage {
        let samples = u16::from(*value);

        *value = (samples
            .saturating_mul(255)
            .saturating_add(COVERAGE_SAMPLES / 2)
            / COVERAGE_SAMPLES) as u8;
    }
}

fn floor_to_i32(value: f32) -> i32 {
    let truncated = value as i32;

    if (truncated as f32) > value {
        truncated.saturating_sub(1)
    } else {
        truncated
    }
}

fn ceil_to_i32(value: f32) -> i32 {
    let truncated = value as i32;

    if (truncated as f32) < value {
        truncated.saturating_add(1)
    } else {
        truncated
    }
}

fn round_to_i32(value: f32) -> i32 {
    if value >= 0.0 {
        floor_to_i32(value + 0.5)
    } else {
        ceil_to_i32(value - 0.5)
    }
}

fn abs_f32(value: f32) -> f32 {
    if value < 0.0 { -value } else { value }
}

#[cfg(test)]
mod tests;
