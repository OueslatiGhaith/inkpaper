use alloc::string::String;

use inkpaper_ui::prelude::*;

// about as many characters as fit before the toggle
const VISIBLE_CHARS: usize = 22;

/// A single-line field for text typed on the [`Keyboard`], with a Show/Hide
/// toggle for secrets. Text is typed at the end, so the end stays in view.
///
/// [`Keyboard`]: crate::components::keyboard::Keyboard
#[component]
pub(crate) struct TextField<'a> {
    text: &'a str,
    shown: bool,
    on_toggle: Listener<ActivateEvent>,
}

impl RenderOnce for TextField<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let count = self.text.chars().count();
        let skipped = count.saturating_sub(VISIBLE_CHARS);

        let mut visible = String::new();
        if skipped > 0 {
            visible.push_str("...");
        }

        for character in self.text.chars().skip(skipped) {
            visible.push(if self.shown { character } else { '*' });
        }

        let toggle = if self.shown { "Hide" } else { "Show" };

        rsx! {
            <div class="w-[424px] h-16 relative rounded-md border-2 border-black">
                <div class="absolute left-4 top-0 w-[310px] h-[60px] flex items-center">
                    <text class="text-xl no-wrap max-lines-1">
                        {visible}
                    </text>
                </div>

                <div class="absolute left-[334px] top-2 w-[2px] h-11 bg-black" />

                <div
                    id="text-field-toggle"
                    on:activate={self.on_toggle}
                    class="absolute left-[336px] top-0 w-[84px] h-[60px] flex items-center justify-center"
                >
                    <text class="text-xl no-wrap">
                        {toggle}
                    </text>
                </div>
            </div>
        }
    }
}
