use skrifa::{
    FontRef, GlyphId16, Tag,
    raw::{
        TableProvider,
        tables::gsub::{MultipleSubstFormat1, SingleSubst, SubstitutionSubtables},
    },
};

use crate::OpenTypeFeature;

pub(super) fn gsub_single_substitution_for_font(
    font: &FontRef<'_>,
    feature: OpenTypeFeature,
    glyph: GlyphId16,
) -> Option<GlyphId16> {
    let gsub = font.gsub().ok()?;
    let feature_list = gsub.feature_list().ok()?;
    let lookup_list = gsub.lookup_list().ok()?;
    let feature_tag = Tag::new(&feature.tag());

    for record in feature_list.feature_records() {
        if record.feature_tag() != feature_tag {
            continue;
        }

        let Ok(feature_table) = record.feature(feature_list.offset_data()) else {
            continue;
        };

        let mut current = glyph;
        let mut changed = false;

        // lookups inside a feature are applied in order.
        for lookup_index in feature_table.lookup_list_indices() {
            let Ok(lookup) = lookup_list.lookups().get(usize::from(lookup_index.get())) else {
                continue;
            };
            let Ok(subtables) = lookup.subtables() else {
                continue;
            };

            // subtables within one lookup are alternatives.
            // apply the first one that handles the current glyph.
            let substituted = match subtables {
                SubstitutionSubtables::Single(iter) => iter
                    .iter()
                    .filter_map(Result::ok)
                    .find_map(|subtable| apply_single_substitution(&subtable, current)),
                SubstitutionSubtables::Multiple(iter) => iter
                    .iter()
                    .filter_map(Result::ok)
                    .find_map(|subtable| apply_multiple_single_output(&subtable, current)),
                _ => None,
            };

            if let Some(next) = substituted {
                current = next;
                changed = true;
            }
        }

        if changed {
            return Some(current);
        }
    }

    None
}

fn apply_single_substitution(substitution: &SingleSubst<'_>, glyph: GlyphId16) -> Option<GlyphId16> {
    match substitution {
        SingleSubst::Format1(table) => {
            table.coverage().ok()?.get(glyph)?;

            // OpenType SingleSubst format 1 adds the signed delta modulo 65536 to
            // the original glyph ID.
            Some(GlyphId16::new(
                glyph.to_u16().wrapping_add_signed(table.delta_glyph_id()),
            ))
        }
        SingleSubst::Format2(table) => {
            let index = table.coverage().ok()?.get(glyph)?;

            table
                .substitute_glyph_ids()
                .get(usize::from(index))
                .map(|gid| gid.get())
        }
    }
}

pub(super) fn apply_multiple_single_output(
    substitution: &MultipleSubstFormat1<'_>,
    glyph: GlyphId16,
) -> Option<GlyphId16> {
    let index = substitution.coverage().ok()?.get(glyph)?;
    let sequence = substitution.sequences().get(usize::from(index)).ok()?;
    let substitutes = sequence.substitute_glyph_ids();

    // MultipleSubstitution is only compatible with this bounded one-glyph API
    // when it produces exactly one glyph.
    // zero outputs means deletion.
    // more than one output requires a real glyph buffer rewrite and is
    // deliberately unsupported here.
    if substitutes.len() != 1 {
        return None;
    }

    Some(substitutes[0].get())
}

pub(super) fn gsub_pair_ligature_for_font(
    font: &FontRef<'_>,
    feature: OpenTypeFeature,
    first: GlyphId16,
    second: GlyphId16,
) -> Option<GlyphId16> {
    let gsub = font.gsub().ok()?;
    let feature_list = gsub.feature_list().ok()?;
    let lookup_list = gsub.lookup_list().ok()?;
    let feature_tag = Tag::new(&feature.tag());

    for record in feature_list.feature_records() {
        if record.feature_tag() != feature_tag {
            continue;
        }

        let Ok(feature_table) = record.feature(feature_list.offset_data()) else {
            continue;
        };

        for lookup_index in feature_table.lookup_list_indices() {
            let Ok(lookup) = lookup_list.lookups().get(usize::from(lookup_index.get())) else {
                continue;
            };
            let Ok(SubstitutionSubtables::Ligature(subtables)) = lookup.subtables() else {
                continue;
            };

            for subtable in subtables.iter().filter_map(Result::ok) {
                let Some(first_index) = subtable.coverage().ok().and_then(|cov| cov.get(first))
                else {
                    continue;
                };
                let Ok(set) = subtable.ligature_sets().get(usize::from(first_index)) else {
                    continue;
                };

                for ligature in set.ligatures().iter().filter_map(Result::ok) {
                    let components = ligature.component_glyph_ids();
                    // this bounded API deliberately handles exactly a two-glyph ligature.
                    if components.len() == 1 && components[0].get() == second {
                        return Some(ligature.ligature_glyph());
                    }
                }
            }
        }
    }

    None
}
