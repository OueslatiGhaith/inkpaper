use icu_properties::{
    CodePointMapData, CodePointMapDataBorrowed,
    props::{CanonicalCombiningClass, GraphemeClusterBreak},
};

use super::arabic::{self, MarkPlacement};

const COMBINING_CLASSES: CodePointMapDataBorrowed<'static, CanonicalCombiningClass> =
    CodePointMapData::<CanonicalCombiningClass>::new();
const GRAPHEME_BREAKS: CodePointMapDataBorrowed<'static, GraphemeClusterBreak> =
    CodePointMapData::<GraphemeClusterBreak>::new();

/// whether `character` continues the grapheme cluster of the character before it, such as
/// a combining accent, a Hebrew point or an Indic vowel sign
pub(super) fn extends_cluster(character: char) -> bool {
    matches!(
        GRAPHEME_BREAKS.get(character),
        GraphemeClusterBreak::Extend | GraphemeClusterBreak::SpacingMark
    )
}

/// where a combining mark sits relative to its base when the font has no anchors for it.
///
/// `None` keeps the mark as a spacing glyph, which is what marks we can't place yet, such
/// as the Hebrew dagesh inside its letter, fall back to
pub(super) fn mark_placement(character: char) -> Option<MarkPlacement> {
    // the Arabic table also tells shadda apart, which stacks closest to its base
    if let Some(placement) = arabic::mark_placement(character) {
        return Some(placement);
    }

    match COMBINING_CLASSES.get(character) {
        CanonicalCombiningClass::Above
        | CanonicalCombiningClass::AboveLeft
        | CanonicalCombiningClass::AboveRight
        | CanonicalCombiningClass::AttachedAbove
        | CanonicalCombiningClass::AttachedAboveRight
        | CanonicalCombiningClass::DoubleAbove
        // Hebrew holam, rafe, shin dot, sin dot and varika
        | CanonicalCombiningClass::CCC19
        | CanonicalCombiningClass::CCC23
        | CanonicalCombiningClass::CCC24
        | CanonicalCombiningClass::CCC25
        | CanonicalCombiningClass::CCC26
        // Arabic fathatan, dammatan, fatha, damma, sukun and superscript alef
        | CanonicalCombiningClass::CCC27
        | CanonicalCombiningClass::CCC28
        | CanonicalCombiningClass::CCC30
        | CanonicalCombiningClass::CCC31
        | CanonicalCombiningClass::CCC34
        | CanonicalCombiningClass::CCC35
        // Syriac superscript alaph
        | CanonicalCombiningClass::CCC36
        // Thai and Lao tone marks
        | CanonicalCombiningClass::CCC107
        | CanonicalCombiningClass::CCC122 => Some(MarkPlacement::Above),

        CanonicalCombiningClass::CCC33 => Some(MarkPlacement::Shadda),

        CanonicalCombiningClass::Below
        | CanonicalCombiningClass::BelowLeft
        | CanonicalCombiningClass::BelowRight
        | CanonicalCombiningClass::AttachedBelow
        | CanonicalCombiningClass::AttachedBelowLeft
        | CanonicalCombiningClass::DoubleBelow
        | CanonicalCombiningClass::IotaSubscript
        // Hebrew sheva, hataf vowels, hiriq, tsere, segol, patah, qamats, qubuts and meteg
        | CanonicalCombiningClass::CCC10
        | CanonicalCombiningClass::CCC11
        | CanonicalCombiningClass::CCC12
        | CanonicalCombiningClass::CCC13
        | CanonicalCombiningClass::CCC14
        | CanonicalCombiningClass::CCC15
        | CanonicalCombiningClass::CCC16
        | CanonicalCombiningClass::CCC17
        | CanonicalCombiningClass::CCC18
        | CanonicalCombiningClass::CCC20
        | CanonicalCombiningClass::CCC22
        // Arabic kasratan and kasra
        | CanonicalCombiningClass::CCC29
        | CanonicalCombiningClass::CCC32
        // Thai and Lao below-base vowels
        | CanonicalCombiningClass::CCC103
        | CanonicalCombiningClass::CCC118 => Some(MarkPlacement::Below),

        _ => None,
    }
}
