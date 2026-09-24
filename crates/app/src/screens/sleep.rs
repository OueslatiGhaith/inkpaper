use inkpaper_ui::prelude::*;

const INKPAPER_LOGO: SvgSource = include_svg!("assets/inkpaper-logo.svg");

#[component]
pub(crate) struct SleepScreen;

impl RenderOnce for SleepScreen {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        rsx! {
            <div class="w-[480px] h-[800px] relative bg-black text-white">
                <div class="absolute left-[176px] top-[328px] w-[128px] h-[128px] flex items-center justify-center">
                    <svg source={INKPAPER_LOGO} class="w-[128px] h-[128px] text-white" />
                </div>

                <div class="absolute left-0 top-[470px] w-[480px] flex justify-center">
                    <text class="font-bold text-2xl text-white">
                        {"InkPaper"}
                    </text>
                </div>

                <div class="absolute left-0 top-[500px] w-[480px] flex justify-center">
                    <text class="text-base text-white">
                        {"SLEEPING"}
                    </text>
                </div>
            </div>
        }
    }
}
