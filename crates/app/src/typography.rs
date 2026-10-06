use inkpaper_ui::{
    FontData, FontFace, FontFamilyId, FontRegistry, FontRegistryError, ResourceRuntimeApi, TtfFont,
};

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

#[cfg(not(feature = "hinting"))]
static UI_FONT: TtfFont<'static> = INTER;

#[cfg(feature = "hinting")]
static UI_FONT: inkpaper_ui::HintedTtfFont<'static> = inkpaper_ui::HintedTtfFont::new(INTER);

#[cfg(not(feature = "hinting"))]
static READER_FONTS: [TtfFont<'static>; 2] = [LIBRON_REGULAR, LIBRON_BOLD];

#[cfg(feature = "hinting")]
static READER_FONTS: [inkpaper_ui::HintedTtfFont<'static>; 2] = [
    inkpaper_ui::HintedTtfFont::new(LIBRON_REGULAR),
    inkpaper_ui::HintedTtfFont::new(LIBRON_BOLD),
];

/// The UI family is registered first, so it is the default family and the
/// glyph fallback for the reader.
pub(crate) const UI_FAMILY: FontFamilyId = FontFamilyId::DEFAULT;

pub(crate) const READER_FAMILY: FontFamilyId = FontFamilyId::new(1);

/// Every registered face, in registration order.
pub(crate) const FONT_FACES: usize = 3;

pub(crate) fn register<'resource>(
    runtime: &mut impl ResourceRuntimeApi<'resource>,
) -> Result<(), FontRegistryError> {
    let ui = runtime.register_font_family()?;
    let reader = runtime.register_font_family()?;

    debug_assert_eq!((ui, reader), (UI_FAMILY, READER_FAMILY));

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

    debug_assert_eq!((ui, reader), (UI_FAMILY, READER_FAMILY));

    fonts.register_face(ui, &UI_FONT)?;

    for face in &READER_FONTS {
        fonts.register_face(reader, face as &dyn FontFace)?;
    }

    Ok(())
}
