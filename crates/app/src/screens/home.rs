use inkpaper_ui::prelude::*;

use crate::{AppModel, theme::Theme};

const PROGRESS_TRACK_WIDTH: i32 = 220;

pub struct HomeScreen<'a> {
    model: &'a AppModel,
    cover: Option<ImageSource>,
}

impl<'a> HomeScreen<'a> {
    pub const fn new(model: &'a AppModel, cover: Option<ImageSource>) -> Self {
        Self { model, cover }
    }
}

impl RenderOnce for HomeScreen<'_> {
    fn render(self, cx: &AppContext<'_>) -> impl IntoElement {
        let theme = cx.global::<Theme>();
        let book = self.model.current_book();

        let progress_width =
            px(i32::from(book.progress().value()).saturating_mul(PROGRESS_TRACK_WIDTH) / 100);

        let cover = match self.cover {
            Some(source) => Either::Left(
                image(source)
                    .size(Size::new(px(110), px(150)))
                    .cover()
                    .sampling(ImageSampling::Bilinear)
                    .monochrome()
                    .dither(ImageDither::Bayer4x4),
            ),

            None => Either::Right(
                div()
                    .w(px(110))
                    .h(px(150))
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(theme.ink)
                    .text_color(theme.paper)
                    .child("BOOK"),
            ),
        };

        div()
            .w_full()
            .p(px(24))
            .gap(px(22))
            .child(
                text("Continue reading")
                    .font(FontId::new(1))
                    .text_color(theme.ink),
            )
            .child(
                div()
                    .w_full()
                    .p(px(18))
                    .gap(px(18))
                    .flex()
                    .items_center()
                    .border(px(2))
                    .border_color(theme.ink)
                    .rounded(px(8))
                    .child(cover)
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
                                    .child(
                                        div()
                                            .w(progress_width)
                                            .h_full()
                                            .bg(theme.ink),
                                    ),
                            )
                            .child(
                                text(book.progress().label())
                                    .text_color(theme.muted),
                            ),
                    ),
            )
            .child(
                div()
                    .w_full()
                    .p(px(18))
                    .gap(px(8))
                    .bg(theme.surface)
                    .rounded(px(8))
                    .child(
                        text("Your reader, your library.")
                            .text_color(theme.ink),
                    )
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
