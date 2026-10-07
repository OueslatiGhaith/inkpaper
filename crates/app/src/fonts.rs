//! Reader fonts on the card, found and loaded like crosspoint's
//! `SdCardFontRegistry`.

use alloc::{format, rc::Rc, string::String, vec::Vec};
use core::fmt;

use inkpaper_ui::{FontFace, FontFamilyId, FontStyle, FontWeight, OwnedTtfFont};

use crate::{
    AppPlatform, PlatformEntry,
    reader::ReaderFont,
    typography::{CARD_FAMILY, READER_FAMILY},
};

/// Scanned in order, so a family in `/.fonts` hides one of the same name in
/// `/fonts`.
const FONT_ROOTS: [&str; 2] = ["/.fonts", "/fonts"];

const MAX_FONT_FAMILIES: usize = 128;

const FONT_EXTENSIONS: [&str; 3] = [".ttf", ".otf", ".ttc"];

/// What a family's chosen faces may take of memory together. Fonts are read
/// whole, so this keeps a family well inside PSRAM.
const MAX_FAMILY_BYTES: usize = 4 * 1024 * 1024;

/// A font family on the card: a loose font file, or a folder of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FontFamily {
    name: String,
    /// the family's font files, by path
    files: Vec<String>,
}

impl FontFamily {
    pub(crate) fn name(&self) -> &str {
        &self.name
    }
}

/// The families in the font folders, sorted by name. A loose font file is a
/// family named after the file; a folder of font files is a family named
/// after the folder.
pub(crate) async fn find_font_families<P: AppPlatform>(platform: &mut P) -> Vec<FontFamily> {
    let mut families: Vec<FontFamily> = Vec::new();

    for root in FONT_ROOTS {
        // a card without the folder has no fonts in it
        let Ok(entries) = platform.list_directory(root).await else {
            continue;
        };

        for entry in entries {
            let Some(family) = font_family(platform, root, &entry).await else {
                continue;
            };

            if families.iter().all(|other| other.name != family.name) {
                families.push(family);
            }
        }
    }

    families.sort_by(|left, right| left.name.cmp(&right.name));
    families.truncate(MAX_FONT_FAMILIES);

    families
}

async fn font_family<P: AppPlatform>(
    platform: &mut P,
    root: &str,
    entry: &PlatformEntry,
) -> Option<FontFamily> {
    let name = entry.name();

    if hidden(name) {
        return None;
    }

    if !entry.is_directory() {
        return Some(FontFamily {
            name: String::from(font_file_stem(name)?),
            files: Vec::from([format!("{root}/{name}")]),
        });
    }

    let folder = format!("{root}/{name}");
    let entries = platform.list_directory(&folder).await.ok()?;

    let mut files: Vec<String> = entries
        .iter()
        .filter(|entry| !entry.is_directory() && !hidden(entry.name()))
        .filter(|entry| font_file_stem(entry.name()).is_some())
        .map(|entry| format!("{folder}/{}", entry.name()))
        .collect();

    files.sort();

    (!files.is_empty()).then(|| FontFamily {
        name: String::from(name),
        files,
    })
}

/// crosspoint skips names starting with `.` or `_`, like macOS's `._` files
fn hidden(name: &str) -> bool {
    name.starts_with(['.', '_'])
}

/// The name without its `.ttf`, `.otf` or `.ttc`, in any case.
fn font_file_stem(name: &str) -> Option<&str> {
    FONT_EXTENSIONS.iter().find_map(|extension| {
        let split = name.len().checked_sub(extension.len())?;
        let (stem, tail) = (name.get(..split)?, name.get(split..)?);

        (!stem.is_empty() && tail.eq_ignore_ascii_case(extension)).then_some(stem)
    })
}

/// A card font's face. Its bytes are shared, so the screen's registry and the
/// reader's measuring use one copy.
pub(crate) type CardFace = OwnedTtfFont<Rc<[u8]>>;

/// A card family read into memory: up to a regular, bold, italic and bold
/// italic face.
#[derive(Clone)]
pub(crate) struct CardFont {
    /// the family's index in the families found at startup
    family: u8,
    faces: Vec<CardFace>,
}

impl CardFont {
    pub(crate) const fn family(&self) -> u8 {
        self.family
    }

    pub(crate) fn faces(&self) -> &[CardFace] {
        &self.faces
    }
}

impl fmt::Debug for CardFont {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CardFont")
            .field("family", &self.family)
            .field("faces", &self.faces.len())
            .finish()
    }
}

/// `card` when `font` is its family, so text lays out and paints with it.
/// Without it a card font that didn't load falls back to the built-in one.
pub(crate) fn card_for(font: ReaderFont, card: Option<&CardFont>) -> Option<&CardFont> {
    card.filter(|card| font == ReaderFont::Card(card.family))
}

/// The family text in `font` paints with, given the loaded `card` font.
pub(crate) fn reader_family(font: ReaderFont, card: Option<&CardFont>) -> FontFamilyId {
    if card_for(font, card).is_some() {
        CARD_FAMILY
    } else {
        READER_FAMILY
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FontLoadError {
    /// no file could be read as a font
    NoFaces,
    /// the chosen faces are over [`MAX_FAMILY_BYTES`]
    TooLarge,
}

/// A face's weight and slant, as its font file declares them.
#[derive(Debug, Clone, Copy)]
struct Candidate {
    file: usize,
    weight: u16,
    italic: bool,
    bytes: usize,
}

/// Reads family `index` and picks its faces like crosspoint: the upright face
/// nearest regular weight, then bold, italic and bold italic ones heavier or
/// slanted from it. Files are read once to learn their faces and again for
/// the chosen ones, so only those stay in memory.
pub(crate) async fn load_card_font<P: AppPlatform>(
    platform: &mut P,
    families: &[FontFamily],
    index: u8,
) -> Result<CardFont, FontLoadError> {
    let family = families
        .get(usize::from(index))
        .ok_or(FontLoadError::NoFaces)?;

    let mut candidates = Vec::new();

    for (file, path) in family.files.iter().enumerate() {
        let Ok(bytes) = platform.read_file(path, MAX_FAMILY_BYTES).await else {
            continue;
        };

        // a collection's first face, like crosspoint
        let Ok(face) = OwnedTtfFont::parse(bytes.as_slice(), 0) else {
            continue;
        };

        candidates.push(Candidate {
            file,
            weight: face.weight_range().default_weight().value(),
            italic: face.style() == FontStyle::Italic,
            bytes: bytes.len(),
        });
    }

    let chosen = choose_faces(&candidates);

    if chosen.is_empty() {
        return Err(FontLoadError::NoFaces);
    }

    if chosen
        .iter()
        .map(|candidate| candidate.bytes)
        .sum::<usize>()
        > MAX_FAMILY_BYTES
    {
        return Err(FontLoadError::TooLarge);
    }

    let mut faces = Vec::new();

    for candidate in chosen {
        let path = &family.files[candidate.file];

        let Ok(bytes) = platform.read_file(path, MAX_FAMILY_BYTES).await else {
            continue;
        };

        if let Ok(face) = OwnedTtfFont::parse(Rc::from(bytes), 0) {
            faces.push(face);
        }
    }

    if faces.is_empty() {
        return Err(FontLoadError::NoFaces);
    }

    Ok(CardFont {
        family: index,
        faces,
    })
}

/// crosspoint's `refineVectorStyles`: regular, bold, italic and bold italic
/// roles, each left out when no face fits it.
fn choose_faces(candidates: &[Candidate]) -> Vec<Candidate> {
    let regular_weight = FontWeight::NORMAL.value();
    let bold_weight = FontWeight::BOLD.value();

    let Some(regular) = pick(candidates, false, regular_weight, None)
        .or_else(|| pick(candidates, true, regular_weight, None))
    else {
        return Vec::new();
    };

    let bold = pick(candidates, false, bold_weight, Some(regular))
        .filter(|bold| bold.weight > regular.weight);

    let italic = if regular.italic {
        None
    } else {
        pick(candidates, true, regular_weight, None)
    };

    let bold_italic = pick(
        candidates,
        true,
        bold_weight,
        Some(italic.unwrap_or(regular)),
    )
    .filter(|bold_italic| bold_italic.italic)
    .filter(|bold_italic| italic.is_none_or(|italic| bold_italic.weight > italic.weight));

    [Some(regular), bold, italic, bold_italic]
        .into_iter()
        .flatten()
        .collect()
}

/// The face in `italic` nearest `weight`, the lighter one on a tie, other
/// than `exclude`.
fn pick(
    candidates: &[Candidate],
    italic: bool,
    weight: u16,
    exclude: Option<Candidate>,
) -> Option<Candidate> {
    candidates
        .iter()
        .filter(|candidate| candidate.italic == italic)
        .filter(|candidate| exclude.is_none_or(|exclude| exclude.file != candidate.file))
        .min_by_key(|candidate| {
            (
                candidate.weight.abs_diff(weight),
                candidate.weight,
                candidate.file,
            )
        })
        .copied()
}
