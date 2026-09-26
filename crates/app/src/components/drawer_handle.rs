use inkpaper_ui::prelude::*;

/// The FreeInk drawer handle bar. Callers position it on their sheet.
#[component]
pub(crate) struct DrawerHandle;

impl RenderOnce for DrawerHandle {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        rsx! {
            <div class="w-[72px] h-[5px] rounded-md bg-black" />
        }
    }
}
