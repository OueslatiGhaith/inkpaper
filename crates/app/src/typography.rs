use inkpaper_ui::{FontData, FontRegistryError, ResourceRuntimeApi, TtfFont};

static UI_REGULAR_FONT: TtfFont<'static> = TtfFont::from_data(
    FontData::new(include_bytes!("../assets/fonts/Inter-Regular.ttf")),
    0,
);
static UI_BOLD_FONT: TtfFont<'static> = TtfFont::from_data(
    FontData::new(include_bytes!("../assets/fonts/Inter-Bold.ttf")),
    0,
);

pub(crate) fn register<'resource>(
    runtime: &mut impl ResourceRuntimeApi<'resource>,
) -> Result<(), FontRegistryError> {
    let family = runtime.register_font_family()?;

    runtime.register_font_face(family, &UI_REGULAR_FONT)?;
    runtime.register_font_face(family, &UI_BOLD_FONT)?;

    Ok(())
}
