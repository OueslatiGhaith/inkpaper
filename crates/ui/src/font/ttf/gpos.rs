use ttf_parser::{
    Face, GlyphId as TtfGlyphId, Tag,
    gpos::{Anchor, PairAdjustment, PositioningSubtable},
};

use crate::{CursiveAttachment, Offset, Pixels, px};

use super::metrics::{font_scale, round_to_i32};

pub(super) fn gpos_cursive_attachment_for_face(
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

pub(super) fn gpos_kerning_for_face(
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

pub(super) fn legacy_kerning_for_face(
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

pub(super) fn mark_to_base_offset_for_face(
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

pub(super) fn mark_to_ligature_offset_for_face(
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

pub(super) fn mark_to_mark_offset_for_face(
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
