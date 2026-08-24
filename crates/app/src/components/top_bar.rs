use inkpaper_ui::prelude::*;

use crate::theme::Theme;

pub struct TopBar<'a> {
    clock: &'a str,
    battery: &'a str,
}

impl<'a> TopBar<'a> {
    pub const fn new(clock: &'a str, battery: &'a str) -> Self {
        Self { clock, battery }
    }
}

impl RenderOnce for TopBar<'_> {
    fn render(self, cx: &AppContext<'_>) -> impl IntoElement {
        let theme = cx.global::<Theme>();

        div()
            .w_full()
            .h(px(58))
            .px(px(20))
            .flex()
            .items_center()
            .bg(theme.paper)
            .border(px(1))
            .border_color(theme.subtle)
            .child(
                div()
                    .flex()
                    .flex_1()
                    .items_center()
                    .child(text("InkPaper").font(FontId::new(1)).text_color(theme.ink)),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .items_center()
                    .justify_center()
                    .child(text(self.clock).text_color(theme.muted)),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .items_center()
                    .justify_end()
                    .child(text(self.battery).text_color(theme.muted)),
            )
    }
}
