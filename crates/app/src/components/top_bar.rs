use inkpaper_ui::prelude::*;

use crate::theme::Theme;

pub struct TopBar<'a> {
    battery: &'a str,
}

impl<'a> TopBar<'a> {
    pub const fn new(battery: &'a str) -> Self {
        Self { battery }
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
            .justify_between()
            .bg(theme.paper)
            .border(px(1))
            .border_color(theme.subtle)
            .child(text("InkPaper").font(FontId::new(1)).text_color(theme.ink))
            .child(text(self.battery).text_color(theme.muted))
    }
}
