use alloc::boxed::Box;

use inkpaper_ui::{
    FontData, FontFace, FontFamilyId, FontRegistry, FontRegistryError, ResourceRuntimeApi, TtfFont,
};

use crate::fonts::CardFont;

const INTER: TtfFont<'static> = TtfFont::from_data(
    FontData::new(include_bytes!("../assets/fonts/InterVariable.ttf")),
    0,
);

const LIBRON_REGULAR: TtfFont<'static> = TtfFont::from_data(
    FontData::new(include_bytes!("../assets/fonts/Libron-Regular.ttf")),
    0,
);

const LIBRON_BOLD: TtfFont<'static> = TtfFont::from_data(
    FontData::new(include_bytes!("../assets/fonts/Libron-Bold.ttf")),
    0,
);

const LIBRON_ITALIC: TtfFont<'static> = TtfFont::from_data(
    FontData::new(include_bytes!("../assets/fonts/Libron-Italic.ttf")),
    0,
);

const LIBRON_BOLD_ITALIC: TtfFont<'static> = TtfFont::from_data(
    FontData::new(include_bytes!("../assets/fonts/Libron-BoldItalic.ttf")),
    0,
);

#[cfg(not(feature = "hinting"))]
static UI_FONT: TtfFont<'static> = INTER;

#[cfg(feature = "hinting")]
static UI_FONT: inkpaper_ui::HintedTtfFont<'static> = inkpaper_ui::HintedTtfFont::new(INTER);

#[cfg(not(feature = "hinting"))]
static READER_FONTS: [TtfFont<'static>; 4] = [
    LIBRON_REGULAR,
    LIBRON_BOLD,
    LIBRON_ITALIC,
    LIBRON_BOLD_ITALIC,
];

#[cfg(feature = "hinting")]
static READER_FONTS: [inkpaper_ui::HintedTtfFont<'static>; 4] = [
    inkpaper_ui::HintedTtfFont::new(LIBRON_REGULAR),
    inkpaper_ui::HintedTtfFont::new(LIBRON_BOLD),
    inkpaper_ui::HintedTtfFont::new(LIBRON_ITALIC),
    inkpaper_ui::HintedTtfFont::new(LIBRON_BOLD_ITALIC),
];

/// The UI family is registered first, so it is the default family and the
/// glyph fallback for the reader.
pub(crate) const UI_FAMILY: FontFamilyId = FontFamilyId::DEFAULT;

pub(crate) const READER_FAMILY: FontFamilyId = FontFamilyId::new(1);

/// The card font the reader uses, empty until one is loaded.
pub(crate) const CARD_FAMILY: FontFamilyId = FontFamilyId::new(2);

/// the reader family's name, as the font settings show it
pub(crate) const READER_FONT_NAME: &str = "Libron";

/// A card font's regular, bold, italic and bold italic faces at most.
const CARD_FACES: usize = 4;

/// Every registered face, in registration order, with room for a card font's.
pub(crate) const FONT_FACES: usize = 5 + CARD_FACES;

pub(crate) fn register<'resource>(
    runtime: &mut impl ResourceRuntimeApi<'resource>,
) -> Result<(), FontRegistryError> {
    let ui = runtime.register_font_family()?;
    let reader = runtime.register_font_family()?;
    let card = runtime.register_font_family()?;

    debug_assert_eq!((ui, reader, card), (UI_FAMILY, READER_FAMILY, CARD_FAMILY));

    runtime.register_font_face(ui, &UI_FONT)?;

    for face in &READER_FONTS {
        runtime.register_font_face(reader, face)?;
    }

    Ok(())
}

/// Registers the same families and faces as [`register`], so the reader
/// measures text with the fallback order it is painted with.
pub(crate) fn register_in<const FONTS: usize>(
    fonts: &mut FontRegistry<'static, FONTS>,
) -> Result<(), FontRegistryError> {
    let ui = fonts.register_family()?;
    let reader = fonts.register_family()?;
    let card = fonts.register_family()?;

    debug_assert_eq!((ui, reader, card), (UI_FAMILY, READER_FAMILY, CARD_FAMILY));

    fonts.register_face(ui, &UI_FONT)?;

    for face in &READER_FONTS {
        fonts.register_face(reader, face as &dyn FontFace)?;
    }

    Ok(())
}

/// Replaces the card family's faces with `font`'s, or empties it.
pub(crate) fn register_card_font<'resource>(
    runtime: &mut impl ResourceRuntimeApi<'resource>,
    font: Option<&CardFont>,
) -> Result<(), FontRegistryError> {
    runtime.clear_owned_font_faces();

    for face in font.map_or(&[][..], CardFont::faces) {
        runtime.register_owned_font_face(CARD_FAMILY, Box::new(face.clone()))?;
    }

    Ok(())
}

/// Registers `font`'s faces like [`register_card_font`], after the faces
/// [`register_in`] registers.
pub(crate) fn register_card_font_in<const FONTS: usize>(
    fonts: &mut FontRegistry<'static, FONTS>,
    font: &CardFont,
) -> Result<(), FontRegistryError> {
    for face in font.faces() {
        fonts.register_owned_face(CARD_FAMILY, Box::new(face.clone()))?;
    }

    Ok(())
}
