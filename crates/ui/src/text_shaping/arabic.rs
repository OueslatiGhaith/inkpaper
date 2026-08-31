#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JoiningType {
    NonJoining,
    Transparent,
    JoinCausing,
    DualJoining,
    RightJoining,
}

impl JoiningType {
    pub(crate) const fn is_transparent(self) -> bool {
        matches!(self, Self::Transparent)
    }

    /// whether this character can connect to the preceding logical arabic character.
    pub(crate) const fn accepts_previous(self) -> bool {
        matches!(
            self,
            Self::JoinCausing | Self::DualJoining | Self::RightJoining
        )
    }

    /// whether this character can continue a joining chain toward the following logical
    /// arabic character
    pub(crate) const fn connects_forward(self) -> bool {
        matches!(self, Self::JoinCausing | Self::DualJoining)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub(crate) enum MarkPlacement {
    /// shadda is handled separately so it stays closest to the base when combined
    /// with another above-base vowel mark.
    Shadda,
    Above,
    Below,
}

pub(crate) fn mark_placement(character: char) -> Option<MarkPlacement> {
    match character as u32 {
        // arabic shadda.
        0x0651 => Some(MarkPlacement::Shadda),
        // common below-base harakat and combining signs.
        0x064D // kasratan
        | 0x0650 // kasra
        | 0x0655 // hamza below
        | 0x0656 // subscript alef
        | 0x065C // vowel sign dot below
        | 0x065F // wavy hamza below
        => Some(MarkPlacement::Below),
        // common above-base harakat and combining signs.
        0x064B..=0x064C // fathatan, dammatan
        | 0x064E..=0x064F // fatha, damma
        | 0x0652..=0x0654 // sukun, maddah, hamza above
        | 0x0657..=0x065B
        | 0x065D..=0x065E
        | 0x0670 // superscript alef
        => Some(MarkPlacement::Above),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy)]
struct ArabicForms {
    base: u16,
    isolated: u16,
    final_form: u16,
    initial: u16,
    medial: u16,
}

impl ArabicForms {
    const fn new(base: u16, isolated: u16, final_form: u16, initial: u16, medial: u16) -> Self {
        Self {
            base,
            isolated,
            final_form,
            initial,
            medial,
        }
    }

    fn presentation(self, joins_previous: bool, joins_next: bool) -> Option<char> {
        let codepoint = match (joins_previous, joins_next) {
            (false, false) => self.isolated,
            (true, false) => self.final_form,
            (false, true) => self.initial,
            (true, true) => self.medial,
        };

        if codepoint == 0 {
            return None;
        }

        char::from_u32(u32::from(codepoint))
    }
}

#[derive(Debug, Clone, Copy)]
struct LamAlefForms {
    alef: u16,
    isolated: u16,
    final_form: u16,
}

impl LamAlefForms {
    const fn new(alef: u16, isolated: u16, final_form: u16) -> Self {
        Self {
            alef,
            isolated,
            final_form,
        }
    }

    fn presentation(self, joins_previous: bool) -> Option<char> {
        let codepoint = if joins_previous {
            self.final_form
        } else {
            self.isolated
        };

        char::from_u32(u32::from(codepoint))
    }
}

/// joining data for U+0600..U+077F
///
/// these ranges are compact representation of the Unicode ArabicShaping joining data used
/// by our presentation forms shaper
pub(crate) fn joining_type(character: char) -> JoiningType {
    match character as u32 {
        // transparent marks / format controls.
        0x0610..=0x061A
        | 0x061C
        | 0x064B..=0x065F
        | 0x0670
        | 0x06D6..=0x06DC
        | 0x06DF..=0x06E4
        | 0x06E7..=0x06E8
        | 0x06EA..=0x06ED
        | 0x070F
        | 0x0711
        | 0x0730..=0x074A => JoiningType::Transparent,
        // tatweel.
        0x0640 => JoiningType::JoinCausing,
        // dual-joining characters.
        0x0620
        | 0x0626
        | 0x0628
        | 0x062A..=0x062E
        | 0x0633..=0x063F
        | 0x0641..=0x0647
        | 0x0649..=0x064A
        | 0x066E..=0x066F
        | 0x0678..=0x0687
        | 0x069A..=0x06BF
        | 0x06C1..=0x06C2
        | 0x06CC
        | 0x06CE
        | 0x06D0..=0x06D1
        | 0x06FA..=0x06FC
        | 0x06FF
        | 0x0712..=0x0714
        | 0x071A..=0x071D
        | 0x071F..=0x0727
        | 0x0729
        | 0x072B
        | 0x072D..=0x072E
        | 0x074E..=0x0758
        | 0x075C..=0x076A
        | 0x076D..=0x0770
        | 0x0772
        | 0x0775..=0x0777
        | 0x077A..=0x077F => JoiningType::DualJoining,
        // right-joining characters.
        0x0622..=0x0625
        | 0x0627
        | 0x0629
        | 0x062F..=0x0632
        | 0x0648
        | 0x0671..=0x0673
        | 0x0675..=0x0677
        | 0x0688..=0x0699
        | 0x06C0
        | 0x06C3..=0x06CB
        | 0x06CD
        | 0x06CF
        | 0x06D2..=0x06D3
        | 0x06D5
        | 0x06EE..=0x06EF
        | 0x0710
        | 0x0715..=0x0719
        | 0x071E
        | 0x0728
        | 0x072A
        | 0x072C
        | 0x072F
        | 0x074D
        | 0x0759..=0x075B
        | 0x076B..=0x076C
        | 0x0771
        | 0x0773..=0x0774
        | 0x0778..=0x0779 => JoiningType::RightJoining,
        _ => JoiningType::NonJoining,
    }
}

pub(crate) fn contextual_form(
    character: char,
    joins_previous: bool,
    joins_next: bool,
) -> Option<char> {
    forms_for(character)?.presentation(joins_previous, joins_next)
}

pub(crate) fn lam_alef_form(alef: char, joins_previous: bool) -> Option<char> {
    lam_alef_for(alef)?.presentation(joins_previous)
}

fn forms_for(character: char) -> Option<ArabicForms> {
    let codepoint = u16::try_from(character as u32).ok()?;

    let mut low = 0usize;
    let mut high = ARABIC_FORMS.len();

    while low < high {
        let middle = low + (high - low) / 2;
        let candidate = ARABIC_FORMS[middle];

        if candidate.base < codepoint {
            low = middle + 1;
        } else if candidate.base > codepoint {
            high = middle;
        } else {
            return Some(candidate);
        }
    }

    None
}

fn lam_alef_for(character: char) -> Option<LamAlefForms> {
    let codepoint = u16::try_from(character as u32).ok()?;

    let mut low = 0usize;
    let mut high = LAM_ALEF_FORMS.len();

    while low < high {
        let middle = low + (high - low) / 2;
        let candidate = LAM_ALEF_FORMS[middle];

        if candidate.alef < codepoint {
            low = middle + 1;
        } else if candidate.alef > codepoint {
            high = middle;
        } else {
            return Some(candidate);
        }
    }

    None
}

// sorted by base codepoint.
//
// the data comes from Unicode Arabic Presentation Forms compatibility decompositions.
// Zero means that contextual form doesn't exist.
const ARABIC_FORMS: [ArabicForms; 76] = [
    ArabicForms::new(0x0621, 0xFE80, 0x0000, 0x0000, 0x0000),
    ArabicForms::new(0x0622, 0xFE81, 0xFE82, 0x0000, 0x0000),
    ArabicForms::new(0x0623, 0xFE83, 0xFE84, 0x0000, 0x0000),
    ArabicForms::new(0x0624, 0xFE85, 0xFE86, 0x0000, 0x0000),
    ArabicForms::new(0x0625, 0xFE87, 0xFE88, 0x0000, 0x0000),
    ArabicForms::new(0x0626, 0xFE89, 0xFE8A, 0xFE8B, 0xFE8C),
    ArabicForms::new(0x0627, 0xFE8D, 0xFE8E, 0x0000, 0x0000),
    ArabicForms::new(0x0628, 0xFE8F, 0xFE90, 0xFE91, 0xFE92),
    ArabicForms::new(0x0629, 0xFE93, 0xFE94, 0x0000, 0x0000),
    ArabicForms::new(0x062A, 0xFE95, 0xFE96, 0xFE97, 0xFE98),
    ArabicForms::new(0x062B, 0xFE99, 0xFE9A, 0xFE9B, 0xFE9C),
    ArabicForms::new(0x062C, 0xFE9D, 0xFE9E, 0xFE9F, 0xFEA0),
    ArabicForms::new(0x062D, 0xFEA1, 0xFEA2, 0xFEA3, 0xFEA4),
    ArabicForms::new(0x062E, 0xFEA5, 0xFEA6, 0xFEA7, 0xFEA8),
    ArabicForms::new(0x062F, 0xFEA9, 0xFEAA, 0x0000, 0x0000),
    ArabicForms::new(0x0630, 0xFEAB, 0xFEAC, 0x0000, 0x0000),
    ArabicForms::new(0x0631, 0xFEAD, 0xFEAE, 0x0000, 0x0000),
    ArabicForms::new(0x0632, 0xFEAF, 0xFEB0, 0x0000, 0x0000),
    ArabicForms::new(0x0633, 0xFEB1, 0xFEB2, 0xFEB3, 0xFEB4),
    ArabicForms::new(0x0634, 0xFEB5, 0xFEB6, 0xFEB7, 0xFEB8),
    ArabicForms::new(0x0635, 0xFEB9, 0xFEBA, 0xFEBB, 0xFEBC),
    ArabicForms::new(0x0636, 0xFEBD, 0xFEBE, 0xFEBF, 0xFEC0),
    ArabicForms::new(0x0637, 0xFEC1, 0xFEC2, 0xFEC3, 0xFEC4),
    ArabicForms::new(0x0638, 0xFEC5, 0xFEC6, 0xFEC7, 0xFEC8),
    ArabicForms::new(0x0639, 0xFEC9, 0xFECA, 0xFECB, 0xFECC),
    ArabicForms::new(0x063A, 0xFECD, 0xFECE, 0xFECF, 0xFED0),
    ArabicForms::new(0x0641, 0xFED1, 0xFED2, 0xFED3, 0xFED4),
    ArabicForms::new(0x0642, 0xFED5, 0xFED6, 0xFED7, 0xFED8),
    ArabicForms::new(0x0643, 0xFED9, 0xFEDA, 0xFEDB, 0xFEDC),
    ArabicForms::new(0x0644, 0xFEDD, 0xFEDE, 0xFEDF, 0xFEE0),
    ArabicForms::new(0x0645, 0xFEE1, 0xFEE2, 0xFEE3, 0xFEE4),
    ArabicForms::new(0x0646, 0xFEE5, 0xFEE6, 0xFEE7, 0xFEE8),
    ArabicForms::new(0x0647, 0xFEE9, 0xFEEA, 0xFEEB, 0xFEEC),
    ArabicForms::new(0x0648, 0xFEED, 0xFEEE, 0x0000, 0x0000),
    ArabicForms::new(0x0649, 0xFEEF, 0xFEF0, 0xFBE8, 0xFBE9),
    ArabicForms::new(0x064A, 0xFEF1, 0xFEF2, 0xFEF3, 0xFEF4),
    ArabicForms::new(0x0671, 0xFB50, 0xFB51, 0x0000, 0x0000),
    ArabicForms::new(0x0677, 0xFBDD, 0x0000, 0x0000, 0x0000),
    ArabicForms::new(0x0679, 0xFB66, 0xFB67, 0xFB68, 0xFB69),
    ArabicForms::new(0x067A, 0xFB5E, 0xFB5F, 0xFB60, 0xFB61),
    ArabicForms::new(0x067B, 0xFB52, 0xFB53, 0xFB54, 0xFB55),
    ArabicForms::new(0x067E, 0xFB56, 0xFB57, 0xFB58, 0xFB59),
    ArabicForms::new(0x067F, 0xFB62, 0xFB63, 0xFB64, 0xFB65),
    ArabicForms::new(0x0680, 0xFB5A, 0xFB5B, 0xFB5C, 0xFB5D),
    ArabicForms::new(0x0683, 0xFB76, 0xFB77, 0xFB78, 0xFB79),
    ArabicForms::new(0x0684, 0xFB72, 0xFB73, 0xFB74, 0xFB75),
    ArabicForms::new(0x0686, 0xFB7A, 0xFB7B, 0xFB7C, 0xFB7D),
    ArabicForms::new(0x0687, 0xFB7E, 0xFB7F, 0xFB80, 0xFB81),
    ArabicForms::new(0x0688, 0xFB88, 0xFB89, 0x0000, 0x0000),
    ArabicForms::new(0x068C, 0xFB84, 0xFB85, 0x0000, 0x0000),
    ArabicForms::new(0x068D, 0xFB82, 0xFB83, 0x0000, 0x0000),
    ArabicForms::new(0x068E, 0xFB86, 0xFB87, 0x0000, 0x0000),
    ArabicForms::new(0x0691, 0xFB8C, 0xFB8D, 0x0000, 0x0000),
    ArabicForms::new(0x0698, 0xFB8A, 0xFB8B, 0x0000, 0x0000),
    ArabicForms::new(0x06A4, 0xFB6A, 0xFB6B, 0xFB6C, 0xFB6D),
    ArabicForms::new(0x06A6, 0xFB6E, 0xFB6F, 0xFB70, 0xFB71),
    ArabicForms::new(0x06A9, 0xFB8E, 0xFB8F, 0xFB90, 0xFB91),
    ArabicForms::new(0x06AD, 0xFBD3, 0xFBD4, 0xFBD5, 0xFBD6),
    ArabicForms::new(0x06AF, 0xFB92, 0xFB93, 0xFB94, 0xFB95),
    ArabicForms::new(0x06B1, 0xFB9A, 0xFB9B, 0xFB9C, 0xFB9D),
    ArabicForms::new(0x06B3, 0xFB96, 0xFB97, 0xFB98, 0xFB99),
    ArabicForms::new(0x06BA, 0xFB9E, 0xFB9F, 0x0000, 0x0000),
    ArabicForms::new(0x06BB, 0xFBA0, 0xFBA1, 0xFBA2, 0xFBA3),
    ArabicForms::new(0x06BE, 0xFBAA, 0xFBAB, 0xFBAC, 0xFBAD),
    ArabicForms::new(0x06C0, 0xFBA4, 0xFBA5, 0x0000, 0x0000),
    ArabicForms::new(0x06C1, 0xFBA6, 0xFBA7, 0xFBA8, 0xFBA9),
    ArabicForms::new(0x06C5, 0xFBE0, 0xFBE1, 0x0000, 0x0000),
    ArabicForms::new(0x06C6, 0xFBD9, 0xFBDA, 0x0000, 0x0000),
    ArabicForms::new(0x06C7, 0xFBD7, 0xFBD8, 0x0000, 0x0000),
    ArabicForms::new(0x06C8, 0xFBDB, 0xFBDC, 0x0000, 0x0000),
    ArabicForms::new(0x06C9, 0xFBE2, 0xFBE3, 0x0000, 0x0000),
    ArabicForms::new(0x06CB, 0xFBDE, 0xFBDF, 0x0000, 0x0000),
    ArabicForms::new(0x06CC, 0xFBFC, 0xFBFD, 0xFBFE, 0xFBFF),
    ArabicForms::new(0x06D0, 0xFBE4, 0xFBE5, 0xFBE6, 0xFBE7),
    ArabicForms::new(0x06D2, 0xFBAE, 0xFBAF, 0x0000, 0x0000),
    ArabicForms::new(0x06D3, 0xFBB0, 0xFBB1, 0x0000, 0x0000),
];

const LAM_ALEF_FORMS: [LamAlefForms; 4] = [
    LamAlefForms::new(0x0622, 0xFEF5, 0xFEF6),
    LamAlefForms::new(0x0623, 0xFEF7, 0xFEF8),
    LamAlefForms::new(0x0625, 0xFEF9, 0xFEFA),
    LamAlefForms::new(0x0627, 0xFEFB, 0xFEFC),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_arabic_joining_types_are_classified() {
        assert_eq!(joining_type('ب'), JoiningType::DualJoining);
        assert_eq!(joining_type('ا'), JoiningType::RightJoining);
        assert_eq!(joining_type('\u{064E}'), JoiningType::Transparent);
        assert_eq!(joining_type('\u{0640}'), JoiningType::JoinCausing);
        assert_eq!(joining_type(' '), JoiningType::NonJoining);
    }

    #[test]
    fn beh_contextual_forms_are_selected() {
        assert_eq!(contextual_form('ب', false, false), Some('\u{FE8F}'));
        assert_eq!(contextual_form('ب', true, false), Some('\u{FE90}'));
        assert_eq!(contextual_form('ب', false, true), Some('\u{FE91}'));
        assert_eq!(contextual_form('ب', true, true), Some('\u{FE92}'));
    }

    #[test]
    fn lam_alef_forms_depend_on_previous_join() {
        assert_eq!(lam_alef_form('ا', false), Some('\u{FEFB}'));
        assert_eq!(lam_alef_form('ا', true), Some('\u{FEFC}'));
    }
}
