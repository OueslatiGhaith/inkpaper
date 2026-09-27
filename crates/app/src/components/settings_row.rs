use inkpaper_ui::prelude::*;

use crate::components::icon::{Icon, IconKind, IconProps};

fn row_background(selected: bool) -> Color {
    if selected {
        Color::rgb(170, 170, 170)
    } else {
        Color::WHITE
    }
}

/// A full-width list row: a label, an optional chevron, and a highlight when
/// selected. Rows take taps and focus only when a listener is given.
#[component]
pub(crate) struct ListRow<'a> {
    id: (&'static str, usize),
    label: &'a str,
    /// Nesting level; each level indents the label by 16 px.
    depth: u8,
    selected: bool,
    chevron: bool,
    on_activate: Option<Listener<ActivateEvent>>,
}

impl RenderOnce for ListRow<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let background = row_background(self.selected);

        let indent = i32::from(self.depth.min(6)) * 16;
        let label_left = px(28 + indent);
        let label_width = px(if self.chevron { 380 } else { 412 } - indent);

        rsx! {
            <div class="w-full h-16 relative">
                <div class="absolute left-5 top-0 w-[440px] h-16 rounded-md bg-{background}" />

                {#if let Some(listener) = self.on_activate}
                    <div
                        id={self.id}
                        on:activate={listener}
                        class="absolute left-5 top-0 w-[440px] h-16 rounded-md"
                    />
                {/if}

                <div class="absolute left-{label_left} top-0 w-{label_width} h-16 flex items-center">
                    <text class="text-xl no-wrap max-lines-1 text-ellipsis">
                        {self.label}
                    </text>
                </div>

                {#if self.chevron}
                    <div class="absolute right-7 top-0 h-16 flex items-center">
                        <Icon kind={IconKind::ChevronRight} size={px(24)} />
                    </div>
                {/if}
            </div>
        }
    }
}

/// A list row with a label and a value on the right. Like [`ListRow`], it takes
/// taps only when a listener is given.
#[component]
pub(crate) struct SettingsValueRow<'a> {
    id: (&'static str, usize),
    label: &'a str,
    value: &'a str,
    selected: bool,
    on_activate: Option<Listener<ActivateEvent>>,
}

impl RenderOnce for SettingsValueRow<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let background = row_background(self.selected);

        rsx! {
            <div class="w-full h-16 relative">
                <div class="absolute left-5 top-0 w-[440px] h-16 rounded-md bg-{background}" />

                {#if let Some(listener) = self.on_activate}
                    <div
                        id={self.id}
                        on:activate={listener}
                        class="absolute left-5 top-0 w-[440px] h-16 rounded-md"
                    />
                {/if}

                <div class="absolute left-7 top-0 w-[290px] h-16 flex items-center">
                    <text class="text-xl no-wrap max-lines-1 text-ellipsis">
                        {self.label}
                    </text>
                </div>

                <div class="absolute right-7 top-0 w-[130px] h-16 flex items-center justify-end">
                    <text class="text-xl no-wrap max-lines-1 text-ellipsis">
                        {self.value}
                    </text>
                </div>
            </div>
        }
    }
}
