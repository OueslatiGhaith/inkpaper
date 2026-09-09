use core::fmt::Write;

use heapless::String;
use inkpaper_ui::prelude::*;

use crate::{AppModel, components::BookCover, reader::ReaderSession, theme::Theme};

const PROGRESS_TRACK_WIDTH: i32 = 220;

pub struct HomeScreen<'a> {
    model: &'a AppModel,
    cover: Option<ImageSource>,
    reader: Option<&'a ReaderSession>,
    continue_reading: Option<Listener<ActivateEvent>>,
}

impl<'a> HomeScreen<'a> {
    pub const fn new(
        model: &'a AppModel,
        cover: Option<ImageSource>,
        reader: Option<&'a ReaderSession>,
        continue_reading: Option<Listener<ActivateEvent>>,
    ) -> Self {
        Self {
            model,
            cover,
            reader,
            continue_reading,
        }
    }
}

impl RenderOnce for HomeScreen<'_> {
    fn render(self, cx: &AppContext<'_>) -> impl IntoElement {
        let theme = cx.global::<Theme>();
        let book = self.model.current_book();

        let progress = match self.reader {
            Some(reader) => Either::Left(ChapterProgress {
                reader,
                color: theme.muted,
            }),
            None => {
                let progress_width =
                    px(i32::from(book.progress().value()) * PROGRESS_TRACK_WIDTH / 100);

                Either::Right(
                    div()
                        .gap(px(10))
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
            }
        };

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
                    .child(progress),
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

struct ChapterProgress<'a> {
    reader: &'a ReaderSession,
    color: Color,
}

impl Element for ChapterProgress<'_> {
    fn mount(self, cx: &mut MountCx<'_>) -> Result<NodeId, MountError> {
        // two 64-bit page counts and this label fit in 80 bytes.
        let mut label = String::<80>::new();
        write!(
            label,
            "Page {} of {} in this chapter",
            self.reader.page_number(),
            self.reader.page_count()
        )
        .expect("chapter page label must fit");

        // mount copies the text into frame storage before this stack buffer expires.
        text(label.as_str()).wrap().text_color(self.color).mount(cx)
    }
}
