use inkpaper_ui::{FontData, FontRegistryError, ResourceRuntimeApi, TtfFont};

static UI_FONT: TtfFont<'static> = TtfFont::from_data(
    FontData::new(include_bytes!("../assets/fonts/InterVariable.ttf")),
    0,
);

pub(crate) fn register<'resource>(
    runtime: &mut impl ResourceRuntimeApi<'resource>,
) -> Result<(), FontRegistryError> {
    let family = runtime.register_font_family()?;

    runtime.register_font_face(family, &UI_FONT)?;

    Ok(())
}
