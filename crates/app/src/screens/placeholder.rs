use inkpaper_ui::prelude::*;

use crate::theme;

pub struct PlaceholderScreen {
    title: &'static str,
    message: &'static str,
}

impl PlaceholderScreen {
    pub const fn new(title: &'static str, message: &'static str) -> Self {
        Self { title, message }
    }
}

impl RenderOnce for PlaceholderScreen {
    fn render(self) -> impl IntoElement {
        div()
            .w_full()
            .p(px(24))
            .gap(px(14))
            .child(text(self.title).font(FontId::new(1)).text_color(theme::INK))
            .child(
                text(self.message)
                    .wrap()
                    .line_height(px(14))
                    .text_color(theme::MUTED),
            )
    }
}
