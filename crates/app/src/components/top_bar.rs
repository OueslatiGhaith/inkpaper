use inkpaper_ui::prelude::*;

use crate::theme;

pub struct TopBar<'a> {
    battery: &'a str,
}

impl<'a> TopBar<'a> {
    pub const fn new(battery: &'a str) -> Self {
        Self { battery }
    }
}

impl RenderOnce for TopBar<'_> {
    fn render(self) -> impl IntoElement {
        div()
            .w_full()
            .h(px(58))
            .px(px(20))
            .flex()
            .items_center()
            .justify_between()
            .bg(theme::PAPER)
            .border(px(1))
            .border_color(theme::SUBTLE)
            .child(text("InkPaper").font(FontId::new(1)).text_color(theme::INK))
            .child(text(self.battery).text_color(theme::MUTED))
    }
}
