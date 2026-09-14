use inkpaper_ui::{FontData, FontId, FontRegistryError, ResourceRuntimeApi, TtfFont};

pub(crate) const UI_REGULAR_FONT_ID: FontId = FontId::DEFAULT;
pub(crate) const UI_BOLD_FONT_ID: FontId = FontId::new(1);

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
    let regular = runtime.register_font(&UI_REGULAR_FONT)?;
    let bold = runtime.register_font(&UI_BOLD_FONT)?;

    assert_eq!(
        regular, UI_REGULAR_FONT_ID,
        "regular UI font must occupy font slot 0"
    );
    assert_eq!(
        bold, UI_BOLD_FONT_ID,
        "regular UI font must occupy font slot 1"
    );

    Ok(())
}
