use ttf_parser::{
    Face, GlyphId as TtfGlyphId, Tag,
    gsub::{SingleSubstitution, SubstitutionSubtable},
};

use crate::OpenTypeFeature;

pub(super) fn gsub_single_substitution_for_face(
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

pub(super) fn apply_single_output_substitution(
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

pub(super) fn gsub_pair_ligature_for_face(
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
