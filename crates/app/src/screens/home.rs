use inkpaper_ui::prelude::*;

use crate::{AppModel, theme};

const PROGRESS_TRACK_WIDTH: i32 = 220;

pub struct HomeScreen<'a> {
    model: &'a AppModel,
}

impl<'a> HomeScreen<'a> {
    pub const fn new(model: &'a AppModel) -> Self {
        Self { model }
    }
}

impl RenderOnce for HomeScreen<'_> {
    fn render(self) -> impl IntoElement {
        let book = self.model.current_book();

        let progress_width =
            px(i32::from(book.progress().value()).saturating_mul(PROGRESS_TRACK_WIDTH) / 100);

        div()
            .w_full()
            .p(px(24))
            .gap(px(22))
            .child(
                text("Continue reading")
                    .font(FontId::new(1))
                    .text_color(theme::INK),
            )
            .child(
                div()
                    .w_full()
                    .p(px(18))
                    .gap(px(18))
                    .flex()
                    .items_center()
                    .border(px(2))
                    .border_color(theme::INK)
                    .rounded(px(8))
                    .child(
                        div()
                            .w(px(110))
                            .h(px(150))
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(theme::INK)
                            .text_color(theme::PAPER)
                            .child("BOOK"),
                    )
                    .child(
                        div()
                            .flex_1()
                            .gap(px(10))
                            .child(
                                text(book.title())
                                    .font(FontId::new(1))
                                    .wrap()
                                    .max_lines(2)
                                    .text_ellipsis()
                                    .text_color(theme::INK),
                            )
                            .child(
                                text(book.author())
                                    .wrap()
                                    .max_lines(2)
                                    .text_color(theme::MUTED),
                            )
                            .child(
                                div()
                                    .w(px(PROGRESS_TRACK_WIDTH))
                                    .h(px(12))
                                    .border(px(1))
                                    .border_color(theme::INK)
                                    .child(
                                        div()
                                            .w(progress_width)
                                            .h_full()
                                            .bg(theme::INK),
                                    ),
                            )
                            .child(
                                text(book.progress().label())
                                    .text_color(theme::MUTED),
                            ),
                    ),
            )
            .child(
                div()
                    .w_full()
                    .p(px(18))
                    .gap(px(8))
                    .bg(theme::SURFACE)
                    .rounded(px(8))
                    .child(
                        text("Your reader, your library.")
                            .text_color(theme::INK),
                    )
                    .child(
                        text(
                            "This screen is rendered by inkpaper-app. The simulator and the X4 Pro firmware only provide the runtime, input, and display.",
                        )
                        .wrap()
                        .line_height(px(14))
                        .text_color(theme::MUTED),
                    ),
            )
    }
}
