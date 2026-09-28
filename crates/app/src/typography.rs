use inkpaper_ui::{FontData, FontFace, FontRegistryError, ResourceRuntimeApi, TtfFont};

const INTER: TtfFont<'static> = TtfFont::from_data(
    FontData::new(include_bytes!("../assets/fonts/InterVariable.ttf")),
    0,
);

#[cfg(not(feature = "hinting"))]
static UI_FONT: TtfFont<'static> = INTER;

#[cfg(feature = "hinting")]
static UI_FONT: inkpaper_ui::HintedTtfFont<'static> = inkpaper_ui::HintedTtfFont::new(INTER);

pub(crate) fn ui_font() -> &'static dyn FontFace {
    &UI_FONT
}

pub(crate) fn register<'resource>(
    runtime: &mut impl ResourceRuntimeApi<'resource>,
) -> Result<(), FontRegistryError> {
    let family = runtime.register_font_family()?;

    runtime.register_font_face(family, &UI_FONT)?;

    Ok(())
}
