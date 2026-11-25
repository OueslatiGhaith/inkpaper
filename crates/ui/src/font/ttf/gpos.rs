use skrifa::{
    FontRef, GlyphId16, Tag,
    raw::{
        TableProvider,
        tables::{
            gpos::{AnchorTable, CursivePosFormat1, PairPos, PositionSubtables},
            kern::SubtableKind,
            layout::LookupFlag,
        },
    },
};

use crate::{CursiveAttachment, Offset, PairPositioning, Pixels, float::round_i32, px};

use super::metrics::font_scale;

pub(super) fn gpos_cursive_attachment_for_font(
    font: &FontRef<'_>,
    visual_left: GlyphId16,
    visual_right: GlyphId16,
    size_px: u16,
    right_to_left: bool,
) -> Option<CursiveAttachment> {
    let scale = font_scale(font, size_px)?;
    let gpos = font.gpos().ok()?;
    let feature_list = gpos.feature_list().ok()?;
    let lookup_list = gpos.lookup_list().ok()?;
    let curs_tag = Tag::new(b"curs");

    for record in feature_list.feature_records() {
        if record.feature_tag() != curs_tag {
            continue;
        }

        let Ok(feature) = record.feature(feature_list.offset_data()) else {
            continue;
        };

        for lookup_index in feature.lookup_list_indices() {
            let Ok(lookup) = lookup_list.lookups().get(usize::from(lookup_index.get())) else {
                continue;
            };

            // supporting the opposite baseline-root direction requires maintaining
            // and reversing an attachment graph.
            // for our bounded shaper, only accept the normal case where lookup direction
            // agrees with the bidi run direction.
            let lookup_rtl = (lookup.lookup_flag().to_bits() & LookupFlag::RIGHT_TO_LEFT.to_bits())
                != 0;
            if lookup_rtl != right_to_left {
                continue;
            }

            let Ok(PositionSubtables::Cursive(subtables)) = lookup.subtables() else {
                continue;
            };

            for subtable in subtables.iter().filter_map(Result::ok) {
                if let Some(attachment) = cursive_in_subtable(
                    &subtable,
                    visual_left,
                    visual_right,
                    scale,
                    right_to_left,
                ) {
                    return Some(attachment);
                }
            }
        }
    }

    None
}

pub(super) fn gpos_kerning_for_font(
    font: &FontRef<'_>,
    left: GlyphId16,
    right: GlyphId16,
    size_px: u16,
) -> Option<Pixels> {
    let scale = font_scale(font, size_px)?;
    let gpos = font.gpos().ok()?;
    let feature_list = gpos.feature_list().ok()?;
    let lookup_list = gpos.lookup_list().ok()?;
    let kern_tag = Tag::new(b"kern");

    for record in feature_list.feature_records() {
        if record.feature_tag() != kern_tag {
            continue;
        }

        let Ok(feature) = record.feature(feature_list.offset_data()) else {
            continue;
        };

        for lookup_index in feature.lookup_list_indices() {
            let Ok(lookup) = lookup_list.lookups().get(usize::from(lookup_index.get())) else {
                continue;
            };

            let Ok(PositionSubtables::Pair(subtables)) = lookup.subtables() else {
                continue;
            };

            for subtable in subtables.iter().filter_map(Result::ok) {
                if let Some(kerning) = pair_kerning_in_subtable(&subtable, left, right, scale) {
                    return Some(kerning);
                }
            }
        }
    }

    None
}

pub(super) fn legacy_kerning_for_font(
    font: &FontRef<'_>,
    left: GlyphId16,
    right: GlyphId16,
    size_px: u16,
) -> Option<Pixels> {
    let scale = font_scale(font, size_px)?;
    let kern = font.kern().ok()?;
    let left = skrifa::GlyphId::from(left);
    let right = skrifa::GlyphId::from(right);

    for subtable in kern.subtables().filter_map(Result::ok) {
        if !subtable.is_horizontal()
            || subtable.is_variable()
            || subtable.is_cross_stream()
            || subtable.is_state_machine()
        {
            continue;
        }

        let value = match subtable.kind().ok() {
            Some(SubtableKind::Format0(format0)) => format0.kerning(left, right),
            Some(SubtableKind::Format2(format2)) => format2.kerning(left, right),
            Some(SubtableKind::Format3(format3)) => format3.kerning(left, right),
            _ => None,
        };

        if let Some(value) = value {
            return Some(px(round_i32(value as f32 * scale)));
        }
    }

    None
}

pub(super) fn mark_to_base_offset_for_font(
    font: &FontRef<'_>,
    base: GlyphId16,
    mark: GlyphId16,
    size_px: u16,
) -> Option<Offset> {
    let scale = font_scale(font, size_px)?;
    let gpos = font.gpos().ok()?;
    let feature_list = gpos.feature_list().ok()?;
    let lookup_list = gpos.lookup_list().ok()?;
    let mark_tag = Tag::new(b"mark");

    for record in feature_list.feature_records() {
        if record.feature_tag() != mark_tag {
            continue;
        }

        let Ok(feature) = record.feature(feature_list.offset_data()) else {
            continue;
        };

        for lookup_index in feature.lookup_list_indices() {
            let Ok(lookup) = lookup_list.lookups().get(usize::from(lookup_index.get())) else {
                continue;
            };

            let Ok(PositionSubtables::MarkToBase(subtables)) = lookup.subtables() else {
                continue;
            };

            for subtable in subtables.iter().filter_map(Result::ok) {
                let Some(mark_index) = subtable.mark_coverage().ok().and_then(|c| c.get(mark))
                else {
                    continue;
                };
                let Some(base_index) = subtable.base_coverage().ok().and_then(|c| c.get(base))
                else {
                    continue;
                };
                let Ok(mark_array) = subtable.mark_array() else {
                    continue;
                };
                let Some(mark_record) = mark_array.mark_records().get(usize::from(mark_index))
                else {
                    continue;
                };
                let Ok(mark_anchor) = mark_record.mark_anchor(mark_array.offset_data()) else {
                    continue;
                };
                let Ok(base_array) = subtable.base_array() else {
                    continue;
                };
                let Ok(base_record) = base_array.base_records().get(usize::from(base_index)) else {
                    continue;
                };
                let Some(Ok(base_anchor)) = base_record
                    .base_anchors(base_array.offset_data())
                    .get(usize::from(mark_record.mark_class()))
                else {
                    continue;
                };

                return Some(anchor_attachment_offset(&base_anchor, &mark_anchor, scale));
            }
        }
    }

    None
}

pub(super) fn mark_to_ligature_offset_for_font(
    font: &FontRef<'_>,
    ligature: GlyphId16,
    component: u16,
    mark: GlyphId16,
    size_px: u16,
) -> Option<Offset> {
    let scale = font_scale(font, size_px)?;
    let gpos = font.gpos().ok()?;
    let feature_list = gpos.feature_list().ok()?;
    let lookup_list = gpos.lookup_list().ok()?;
    let mark_tag = Tag::new(b"mark");

    for record in feature_list.feature_records() {
        if record.feature_tag() != mark_tag {
            continue;
        }

        let Ok(feature) = record.feature(feature_list.offset_data()) else {
            continue;
        };

        for lookup_index in feature.lookup_list_indices() {
            let Ok(lookup) = lookup_list.lookups().get(usize::from(lookup_index.get())) else {
                continue;
            };

            let Ok(PositionSubtables::MarkToLig(subtables)) = lookup.subtables() else {
                continue;
            };

            for subtable in subtables.iter().filter_map(Result::ok) {
                let Some(mark_index) = subtable.mark_coverage().ok().and_then(|c| c.get(mark))
                else {
                    continue;
                };
                let Some(ligature_index) = subtable
                    .ligature_coverage()
                    .ok()
                    .and_then(|c| c.get(ligature))
                else {
                    continue;
                };
                let Ok(mark_array) = subtable.mark_array() else {
                    continue;
                };
                let Some(mark_record) = mark_array.mark_records().get(usize::from(mark_index))
                else {
                    continue;
                };
                let Ok(mark_anchor) = mark_record.mark_anchor(mark_array.offset_data()) else {
                    continue;
                };
                let Ok(ligature_array) = subtable.ligature_array() else {
                    continue;
                };
                let Ok(attach) = ligature_array
                    .ligature_attaches()
                    .get(usize::from(ligature_index))
                else {
                    continue;
                };
                let Ok(component_record) = attach.component_records().get(usize::from(component))
                else {
                    continue;
                };
                let Some(Ok(ligature_anchor)) = component_record
                    .ligature_anchors(attach.offset_data())
                    .get(usize::from(mark_record.mark_class()))
                else {
                    continue;
                };

                return Some(anchor_attachment_offset(
                    &ligature_anchor,
                    &mark_anchor,
                    scale,
                ));
            }
        }
    }

    None
}

pub(super) fn mark_to_mark_offset_for_font(
    font: &FontRef<'_>,
    base_mark: GlyphId16,
    mark: GlyphId16,
    size_px: u16,
) -> Option<Offset> {
    let scale = font_scale(font, size_px)?;
    let gpos = font.gpos().ok()?;
    let feature_list = gpos.feature_list().ok()?;
    let lookup_list = gpos.lookup_list().ok()?;
    let mkmk_tag = Tag::new(b"mkmk");

    for record in feature_list.feature_records() {
        if record.feature_tag() != mkmk_tag {
            continue;
        }

        let Ok(feature) = record.feature(feature_list.offset_data()) else {
            continue;
        };

        for lookup_index in feature.lookup_list_indices() {
            let Ok(lookup) = lookup_list.lookups().get(usize::from(lookup_index.get())) else {
                continue;
            };

            let Ok(PositionSubtables::MarkToMark(subtables)) = lookup.subtables() else {
                continue;
            };

            for subtable in subtables.iter().filter_map(Result::ok) {
                // mark1 is the child mark being positioned.
                let Some(mark_index) = subtable.mark1_coverage().ok().and_then(|c| c.get(mark))
                else {
                    continue;
                };
                // mark2 is the already-positioned attachment mark.
                let Some(base_index) = subtable
                    .mark2_coverage()
                    .ok()
                    .and_then(|c| c.get(base_mark))
                else {
                    continue;
                };
                let Ok(mark1_array) = subtable.mark1_array() else {
                    continue;
                };
                let Some(mark_record) = mark1_array.mark_records().get(usize::from(mark_index))
                else {
                    continue;
                };
                let Ok(mark_anchor) = mark_record.mark_anchor(mark1_array.offset_data()) else {
                    continue;
                };
                let Ok(mark2_array) = subtable.mark2_array() else {
                    continue;
                };
                let Ok(mark2_record) = mark2_array.mark2_records().get(usize::from(base_index))
                else {
                    continue;
                };
                let Some(Ok(base_anchor)) = mark2_record
                    .mark2_anchors(mark2_array.offset_data())
                    .get(usize::from(mark_record.mark_class()))
                else {
                    continue;
                };

                return Some(anchor_attachment_offset(&base_anchor, &mark_anchor, scale));
            }
        }
    }

    None
}

pub(super) fn gpos_pair_positioning_for_font(
    font: &FontRef<'_>,
    visual_left: GlyphId16,
    visual_right: GlyphId16,
    size_px: u16,
    right_to_left: bool,
) -> Option<PairPositioning> {
    let scale = font_scale(font, size_px)?;
    let gpos = font.gpos().ok()?;
    let feature_list = gpos.feature_list().ok()?;
    let lookup_list = gpos.lookup_list().ok()?;

    let curs_tag = Tag::new(b"curs");
    let kern_tag = Tag::new(b"kern");

    // cursive positioning has precedence over ordinary pair kerning.
    //
    // we may encounter a kern feature before a curs feature in the font, so remember
    // the first matching kerning adjustment while continuing to look for a cursive attachment.
    let mut kerning = None;

    for record in feature_list.feature_records() {
        let tag = record.feature_tag();

        if tag == curs_tag {
            let Ok(feature) = record.feature(feature_list.offset_data()) else {
                continue;
            };

            for lookup_index in feature.lookup_list_indices() {
                let Ok(lookup) = lookup_list.lookups().get(usize::from(lookup_index.get())) else {
                    continue;
                };

                let lookup_rtl = (lookup.lookup_flag().to_bits()
                    & LookupFlag::RIGHT_TO_LEFT.to_bits())
                    != 0;
                if lookup_rtl != right_to_left {
                    continue;
                }

                let Ok(PositionSubtables::Cursive(subtables)) = lookup.subtables() else {
                    continue;
                };

                for subtable in subtables.iter().filter_map(Result::ok) {
                    if let Some(attachment) = cursive_in_subtable(
                        &subtable,
                        visual_left,
                        visual_right,
                        scale,
                        right_to_left,
                    ) {
                        return Some(PairPositioning::Cursive(attachment));
                    }
                }
            }

            continue;
        }

        if tag != kern_tag || kerning.is_some() {
            continue;
        }

        let Ok(feature) = record.feature(feature_list.offset_data()) else {
            continue;
        };

        'kern_feature: for lookup_index in feature.lookup_list_indices() {
            let Ok(lookup) = lookup_list.lookups().get(usize::from(lookup_index.get())) else {
                continue;
            };

            let Ok(PositionSubtables::Pair(subtables)) = lookup.subtables() else {
                continue;
            };

            for subtable in subtables.iter().filter_map(Result::ok) {
                if let Some(adjustment) =
                    pair_kerning_in_subtable(&subtable, visual_left, visual_right, scale)
                {
                    kerning = Some(adjustment);
                    break 'kern_feature;
                }
            }
        }
    }

    kerning.map(PairPositioning::Kerning)
}

fn cursive_in_subtable(
    subtable: &CursivePosFormat1<'_>,
    visual_left: GlyphId16,
    visual_right: GlyphId16,
    scale: f32,
    right_to_left: bool,
) -> Option<CursiveAttachment> {
    let coverage = subtable.coverage().ok()?;
    let left_index = coverage.get(visual_left)?;
    let right_index = coverage.get(visual_right)?;

    let records = subtable.entry_exit_record();
    let left_record = records.get(usize::from(left_index))?;
    let right_record = records.get(usize::from(right_index))?;
    let data = subtable.offset_data();

    let origin_delta = if right_to_left {
        // visual order:
        //     logical second | logical first
        //        left              right
        // the exit anchor of the logical first glyph attaches to the entry
        // anchor of logical second.
        let left_entry = left_record.entry_anchor(data)?.ok()?;
        let right_exit = right_record.exit_anchor(data)?.ok()?;

        anchor_attachment_offset(&left_entry, &right_exit, scale)
    } else {
        // LTR visual order is also logical order:
        //     left.exit == right.entry
        let left_exit = left_record.exit_anchor(data)?.ok()?;
        let right_entry = right_record.entry_anchor(data)?.ok()?;

        anchor_attachment_offset(&left_exit, &right_entry, scale)
    };

    Some(CursiveAttachment::new(origin_delta))
}

fn pair_kerning_in_subtable(
    subtable: &PairPos<'_>,
    left: GlyphId16,
    right: GlyphId16,
    scale: f32,
) -> Option<Pixels> {
    let x_advance = match subtable {
        PairPos::Format1(table) => {
            let left_index = table.coverage().ok()?.get(left)?;
            let pair_set = table.pair_sets().get(usize::from(left_index)).ok()?;
            let records = pair_set.pair_value_records();

            // OpenType PairSet records are sorted by second_glyph.
            let mut low = 0usize;
            let mut high = records.len();
            let mut found = None;

            while low < high {
                let mid = low + (high - low) / 2;
                let record = records.get(mid).ok()?;
                let candidate = record.second_glyph();

                if candidate < right {
                    low = mid + 1;
                } else if candidate > right {
                    high = mid;
                } else {
                    found = Some(record.value_record1().x_advance().unwrap_or(0));
                    break;
                }
            }

            found?
        }
        PairPos::Format2(table) => {
            table.coverage().ok()?.get(left)?;

            let left_class = table.class_def1().ok()?.get(left);
            let right_class = table.class_def2().ok()?.get(right);
            let class1 = table.class1_records().get(usize::from(left_class)).ok()?;
            let class2 = class1.class2_records().get(usize::from(right_class)).ok()?;

            class2.value_record1().x_advance().unwrap_or(0)
        }
    };

    // InkPaper's current FontFace::kerning contract is a scalar horizontal pair
    // advance adjustment.
    // OpenType kerning conventionally expresses this in the first glyph's
    // xAdvance value.
    // placement fields and the second glyph's advance are deliberately not
    // folded into this scalar because doing so would change their semantics.
    Some(px(round_i32(f32::from(x_advance) * scale)))
}

fn anchor_attachment_offset(parent: &AnchorTable<'_>, child: &AnchorTable<'_>, scale: f32) -> Offset {
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
    let x_units = i32::from(parent.x_coordinate()) - i32::from(child.x_coordinate());
    let y_units = i32::from(child.y_coordinate()) - i32::from(parent.y_coordinate());

    Offset::new(
        px(round_i32(x_units as f32 * scale)),
        px(round_i32(y_units as f32 * scale)),
    )
}
