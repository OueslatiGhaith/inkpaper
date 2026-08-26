use inkpaper_ui::prelude::*;

use crate::{LibraryEntry, LibraryPage, theme::Theme};

const PROGRESS_TRACK_WIDTH: i32 = 180;

pub struct LibraryScreen<'a> {
    library: &'a LibraryPage,
}

impl<'a> LibraryScreen<'a> {
    pub const fn new(library: &'a LibraryPage) -> Self {
        Self { library }
    }
}

impl RenderOnce for LibraryScreen<'_> {
    fn render(self, cx: &AppContext<'_>) -> impl IntoElement {
        let theme = *cx.global::<Theme>();

        let content = if self.library.is_empty() {
            Either::Left(empty_library(theme))
        } else {
            Either::Right(
                div().w_full().gap(px(12)).children(
                    self.library
                        .entries()
                        .iter()
                        .map(move |entry| library_entry(entry, theme)),
                ),
            )
        };

        div()
            .w_full()
            .p(px(24))
            .gap(px(18))
            .child(text("Library").font(FontId::new(1)).text_color(theme.ink))
            .child(content)
    }
}

fn empty_library(theme: Theme) -> impl IntoElement {
    div()
        .w_full()
        .p(px(24))
        .gap(px(8))
        .border(px(1))
        .border_color(theme.subtle)
        .rounded(px(8))
        .child(text("No books yet").text_color(theme.ink))
        .child(
            text("Books placed in /Books will appear here.")
                .wrap()
                .text_color(theme.muted),
        )
}

fn library_entry<'a>(entry: &'a LibraryEntry, theme: Theme) -> impl IntoElement + 'a {
    let book = entry.summary();
    let author = if book.author().is_empty() {
        "Unknown author"
    } else {
        book.author()
    };

    let progress_width =
        px(i32::from(book.progress().value()).saturating_mul(PROGRESS_TRACK_WIDTH) / 100);

    div()
        .w_full()
        .p(px(14))
        .gap(px(14))
        .flex()
        .items_center()
        .border(px(1))
        .border_color(theme.subtle)
        .rounded(px(6))
        .child(
            div()
                .w(px(62))
                .h(px(84))
                .flex()
                .items_center()
                .justify_center()
                .bg(theme.ink)
                .text_color(theme.paper)
                .child("BOOK"),
        )
        .child(
            div()
                .flex_1()
                .gap(px(6))
                .child(
                    text(book.title())
                        .font(FontId::new(1))
                        .wrap()
                        .max_lines(2)
                        .text_ellipsis()
                        .text_color(theme.ink),
                )
                .child(
                    text(author)
                        .wrap()
                        .max_lines(1)
                        .text_ellipsis()
                        .text_color(theme.muted),
                )
                .child(
                    div()
                        .w(px(PROGRESS_TRACK_WIDTH))
                        .h(px(10))
                        .border(px(1))
                        .border_color(theme.ink)
                        .child(div().w(progress_width).h_full().bg(theme.ink)),
                )
                .child(text(book.progress().label()).text_color(theme.muted)),
        )
}
