use inkpaper_ui::prelude::*;

use crate::{AppModel, components::BookCover, theme::Theme};

const PROGRESS_TRACK_WIDTH: i32 = 220;

pub struct HomeScreen<'a> {
    model: &'a AppModel,
    cover: Option<ImageSource>,
    continue_reading: Option<Listener<ActivateEvent>>,
}

impl<'a> HomeScreen<'a> {
    pub const fn new(
        model: &'a AppModel,
        cover: Option<ImageSource>,
        continue_reading: Option<Listener<ActivateEvent>>,
    ) -> Self {
        Self {
            model,
            cover,
            continue_reading,
        }
    }
}

impl RenderOnce for HomeScreen<'_> {
    fn render(self, cx: &AppContext<'_>) -> impl IntoElement {
        let theme = cx.global::<Theme>();
        let book = self.model.current_book();

        let progress_width =
            px(i32::from(book.progress().value()).saturating_mul(PROGRESS_TRACK_WIDTH) / 100);

        let continue_reading = div()
            .w_full()
            .p(px(18))
            .gap(px(18))
            .flex()
            .items_center()
            .border(px(2))
            .border_color(theme.ink)
            .rounded(px(8))
            .child(BookCover::new(self.cover, Size::new(px(110), px(150))))
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
                            .text_color(theme.ink),
                    )
                    .child(
                        text(book.author())
                            .wrap()
                            .max_lines(2)
                            .text_color(theme.muted),
                    )
                    .child(
                        div()
                            .w(px(PROGRESS_TRACK_WIDTH))
                            .h(px(12))
                            .border(px(1))
                            .border_color(theme.ink)
                            .child(div().w(progress_width).h_full().bg(theme.ink)),
                    )
                    .child(text(book.progress().label()).text_color(theme.muted)),
            )
            .when_some(self.continue_reading, |card, listener| {
                card.id("continue-reading")
                    .on_activate(listener)
                    .when_focused(|style| style.border(px(4)))
            });

        div()
            .w_full()
            .p(px(24))
            .gap(px(22))
            .child(
                text("Continue reading")
                    .font(FontId::new(1))
                    .text_color(theme.ink),
            )
            .child(continue_reading)
            .child(
                div()
                    .w_full()
                    .p(px(18))
                    .gap(px(8))
                    .bg(theme.surface)
                    .rounded(px(8))
                    .child(text("Your reader, your library.").text_color(theme.ink))
                    .child(
                        text(
                            "This screen is rendered by inkpaper-app. The simulator and the X4 Pro firmware only provide the runtime, input, and display.",
                        )
                        .wrap()
                        .line_height(px(14))
                        .text_color(theme.muted),
                    ),
            )
    }
}
