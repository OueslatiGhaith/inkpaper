use core::ops::Range;

use icu_properties::{
    CodePointMapData, CodePointMapDataBorrowed,
    props::{BidiClass, BidiMirroringGlyph},
};

use crate::{FontRegistry, Offset};

use super::{ShapedGlyph, TextDirection};

const BIDI_CLASSES: CodePointMapDataBorrowed<'static, BidiClass> =
    CodePointMapData::<BidiClass>::new();
const MIRRORING_GLYPHS: CodePointMapDataBorrowed<'static, BidiMirroringGlyph> =
    CodePointMapData::<BidiMirroringGlyph>::new();

/// One visual run of a line, covering UTF-8 line byte offsets in `range`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct VisualRun {
    pub(super) range: Range<usize>,
    pub(super) right_to_left: bool,
}

/// Visual runs of a line in display (left-to-right) order.
pub(super) enum VisualRuns {
    /// Fast path for pure LTR lines: zero allocations, single pass.
    SingleLtr,
    #[cfg(feature = "bidi")]
    Multi(alloc::vec::Vec<VisualRun>),
}

impl VisualRuns {
    pub(super) fn as_slice(&self) -> &[VisualRun] {
        static SINGLE_LTR: [VisualRun; 1] = [VisualRun {
            range: 0..usize::MAX,
            right_to_left: false,
        }];

        match self {
            Self::SingleLtr => &SINGLE_LTR,
            #[cfg(feature = "bidi")]
            Self::Multi(runs) => runs.as_slice(),
        }
    }
}

/// Whether `character` can produce any non-zero UAX #9 embedding level or isolate.
fn needs_bidi_char(character: char) -> bool {
    matches!(
        BIDI_CLASSES.get(character),
        BidiClass::RightToLeft
            | BidiClass::ArabicLetter
            | BidiClass::RightToLeftEmbedding
            | BidiClass::RightToLeftOverride
            | BidiClass::RightToLeftIsolate
            | BidiClass::LeftToRightEmbedding
            | BidiClass::LeftToRightOverride
            | BidiClass::LeftToRightIsolate
            | BidiClass::FirstStrongIsolate
    )
}

fn needs_bidi(text: &str) -> bool {
    !text.is_ascii() && text.chars().any(needs_bidi_char)
}

/// The direction of a line's text (UAX #9 P2/P3).
pub(crate) fn paragraph_direction(text: &str) -> TextDirection {
    if !needs_bidi(text) {
        return TextDirection::LeftToRight;
    }

    #[cfg(feature = "bidi")]
    {
        let info = unicode_bidi::ParagraphBidiInfo::new_with_data_source(
            &IcuBidiDataSource,
            text,
            None,
        );
        if info.paragraph_level.is_rtl() {
            TextDirection::RightToLeft
        } else {
            TextDirection::LeftToRight
        }
    }

    #[cfg(not(feature = "bidi"))]
    {
        for character in text.chars() {
            match BIDI_CLASSES.get(character) {
                BidiClass::LeftToRight => return TextDirection::LeftToRight,
                BidiClass::RightToLeft | BidiClass::ArabicLetter => {
                    return TextDirection::RightToLeft;
                }
                _ => {}
            }
        }
        TextDirection::LeftToRight
    }
}

/// Computes visual runs for `text` followed by `suffix` (e.g. an ellipsis) in
/// display (left-to-right) order.
///
/// Offsets in `0..text.len()` address `text`, and offsets in
/// `text.len()..text.len() + suffix.len()` address `suffix`.
pub(super) fn resolve_visual_runs(text: &str, suffix: &str) -> VisualRuns {
    if !needs_bidi(text) && !needs_bidi(suffix) {
        return VisualRuns::SingleLtr;
    }

    #[cfg(feature = "bidi")]
    {
        resolve_visual_runs_unicode_bidi(text, suffix)
    }

    #[cfg(not(feature = "bidi"))]
    {
        VisualRuns::SingleLtr
    }
}

#[cfg(feature = "bidi")]
fn resolve_visual_runs_unicode_bidi(text: &str, suffix: &str) -> VisualRuns {
    use alloc::{string::String, vec::Vec};
    use unicode_bidi::ParagraphBidiInfo;

    let owned;
    let combined = if suffix.is_empty() {
        text
    } else {
        owned = {
            let mut s = String::with_capacity(text.len() + suffix.len());
            s.push_str(text);
            s.push_str(suffix);
            s
        };
        owned.as_str()
    };

    if combined.is_empty() {
        return VisualRuns::SingleLtr;
    }

    let info =
        ParagraphBidiInfo::new_with_data_source(&IcuBidiDataSource, combined, None);
    let (levels, runs) = info.visual_runs(0..combined.len());

    let mut visual = Vec::with_capacity(runs.len());
    for range in runs {
        if range.is_empty() {
            continue;
        }
        let right_to_left = levels[range.start].is_rtl();
        visual.push(VisualRun {
            range,
            right_to_left,
        });
    }

    if visual.is_empty() {
        VisualRuns::SingleLtr
    } else {
        VisualRuns::Multi(visual)
    }
}

#[cfg(feature = "bidi")]
struct IcuBidiDataSource;

#[cfg(feature = "bidi")]
impl unicode_bidi::BidiDataSource for IcuBidiDataSource {
    fn bidi_class(&self, c: char) -> unicode_bidi::BidiClass {
        use unicode_bidi::BidiClass as Ub;

        match BIDI_CLASSES.get(c) {
            BidiClass::LeftToRight => Ub::L,
            BidiClass::RightToLeft => Ub::R,
            BidiClass::ArabicLetter => Ub::AL,
            BidiClass::EuropeanNumber => Ub::EN,
            BidiClass::EuropeanSeparator => Ub::ES,
            BidiClass::EuropeanTerminator => Ub::ET,
            BidiClass::ArabicNumber => Ub::AN,
            BidiClass::CommonSeparator => Ub::CS,
            BidiClass::ParagraphSeparator => Ub::B,
            BidiClass::SegmentSeparator => Ub::S,
            BidiClass::WhiteSpace => Ub::WS,
            BidiClass::OtherNeutral => Ub::ON,
            BidiClass::LeftToRightEmbedding => Ub::LRE,
            BidiClass::LeftToRightOverride => Ub::LRO,
            BidiClass::RightToLeftEmbedding => Ub::RLE,
            BidiClass::RightToLeftOverride => Ub::RLO,
            BidiClass::PopDirectionalFormat => Ub::PDF,
            BidiClass::NonspacingMark => Ub::NSM,
            BidiClass::BoundaryNeutral => Ub::BN,
            BidiClass::FirstStrongIsolate => Ub::FSI,
            BidiClass::LeftToRightIsolate => Ub::LRI,
            BidiClass::RightToLeftIsolate => Ub::RLI,
            BidiClass::PopDirectionalIsolate => Ub::PDI,
            _ => Ub::ON,
        }
    }

    fn bidi_matched_opening_bracket(
        &self,
        c: char,
    ) -> Option<unicode_bidi::data_source::BidiMatchedOpeningBracket> {
        use icu_properties::props::BidiPairedBracketType;
        use unicode_bidi::data_source::BidiMatchedOpeningBracket;

        let mirroring = MIRRORING_GLYPHS.get(c);
        match mirroring.paired_bracket_type {
            BidiPairedBracketType::Open => Some(BidiMatchedOpeningBracket {
                opening: canonical_opening_bracket(c),
                is_open: true,
            }),
            BidiPairedBracketType::Close => {
                let opening = mirroring.mirroring_glyph?;
                Some(BidiMatchedOpeningBracket {
                    opening: canonical_opening_bracket(opening),
                    is_open: false,
                })
            }
            _ => None,
        }
    }
}

/// U+2329 is canonically equivalent to U+3008 per BidiBrackets.txt.
#[cfg(feature = "bidi")]
const fn canonical_opening_bracket(character: char) -> char {
    match character {
        '\u{2329}' => '\u{3008}',
        _ => character,
    }
}

/// a glyph's cluster in the piece it was shaped from, given its offset in the line
pub(super) fn piece_cluster(text: &str, line_offset: usize) -> usize {
    if line_offset >= text.len() {
        line_offset - text.len()
    } else {
        line_offset
    }
}

/// mirrors the characters of whole clusters of a right-to-left run, in logical order
/// with their line offsets as clusters. Glyphs after the text are left alone
pub(super) fn mirror_clusters<const FONTS: usize>(
    registry: &FontRegistry<'_, FONTS>,
    size_px: u16,
    text: &str,
    glyphs: &mut [ShapedGlyph],
) {
    let mut previous_cluster = None;

    for glyph in glyphs.iter_mut() {
        let shaped = *glyph;
        let cluster = shaped.cluster();

        // multiple glyphs can share a shaping cluster, such as an Arabic base
        // + transparent mark. Mirroring is a source-character operation,
        // so process the cluster once.
        if previous_cluster == Some(cluster) {
            continue;
        }

        previous_cluster = Some(cluster);

        let Some(remaining) = text.get(cluster..) else {
            continue;
        };
        let Some(character) = remaining.chars().next() else {
            continue;
        };
        let Some(mirrored) = MIRRORING_GLYPHS.get(character).mirroring_glyph else {
            continue;
        };

        // prefer the face that produced the original glyph, then use the normal
        // registered fallback order.
        // do not use replacement fallback here: if the actual mirrored character
        // doesn't exist, keeping the original glyph is better than rendering '?'.
        let Some((resolved, base_advance)) = registry.resolve_character_with_advance_exact(
            shaped.font_instance(),
            mirrored,
            size_px,
        ) else {
            continue;
        };

        *glyph = ShapedGlyph::new_with_instance(
            resolved.font_instance(),
            resolved.glyph(),
            shaped.cluster(),
            base_advance,
            Offset::ZERO,
            base_advance,
        );
    }
}

pub(super) fn reverse_glyph_clusters(glyphs: &mut [ShapedGlyph]) {
    glyphs.reverse();

    let mut start = 0usize;

    while start < glyphs.len() {
        let cluster = glyphs[start].cluster();
        let mut end = start.saturating_add(1);

        while end < glyphs.len() && glyphs[end].cluster() == cluster {
            end = end.saturating_add(1);
        }

        // reversing the complete span changed both cluster order and the order of glyphs
        // inside each cluster. Reverse each cluster again so only the cluster sequence changes.
        glyphs[start..end].reverse();

        start = end;
    }
}
