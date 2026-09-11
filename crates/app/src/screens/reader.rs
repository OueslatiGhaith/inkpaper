use inkpaper_ui::prelude::*;

use crate::{reader::ReaderSession, theme::Theme};

pub struct ReaderScreen<'a> {
    session: &'a ReaderSession,
    page: Canvas,
    previous: Listener<ActivateEvent>,
    next: Listener<ActivateEvent>,
}

impl<'a> ReaderScreen<'a> {
    pub const fn new(
        session: &'a ReaderSession,
        page: Canvas,
        previous: Listener<ActivateEvent>,
        next: Listener<ActivateEvent>,
    ) -> Self {
        Self {
            session,
            page,
            previous,
            next,
        }
    }
}

impl RenderOnce for ReaderScreen<'_> {
    fn render(self, cx: &AppContext<'_>) -> impl IntoElement {
        let theme = *cx.global::<Theme>();

        div()
            .relative()
            .w_full()
            .h_full()
            .overflow_hidden()
            .child(self.page)
            .child(
                div()
                    .absolute()
                    .left(px(0))
                    .top(px(0))
                    .w_full()
                    .h_full()
                    .flex()
                    .child(
                        div()
                            .id("reader-previous-page")
                            .flex_1()
                            .h_full()
                            .on_activate(self.previous),
                    )
                    .child(
                        div()
                            .id("reader-next-page")
                            .flex_1()
                            .h_full()
                            .on_activate(self.next),
                    ),
            )
            .when_some(self.session.notice(), |root, notice| {
                root.child(
                    div()
                        .absolute()
                        .left(px(12))
                        .right(px(12))
                        .bottom(px(12))
                        .p(px(12))
                        .border(px(2))
                        .border_color(theme.ink)
                        .bg(theme.paper)
                        .child(text(notice.message()).wrap().text_color(theme.ink)),
                )
            })
    }
}
