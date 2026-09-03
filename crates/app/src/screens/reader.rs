use inkpaper_ui::prelude::*;

use crate::reader::{ReaderPageView, ReaderSession};

pub struct ReaderScreen<'a> {
    session: &'a ReaderSession,
    previous: Listener<ActivateEvent>,
    next: Listener<ActivateEvent>,
}

impl<'a> ReaderScreen<'a> {
    pub const fn new(
        session: &'a ReaderSession,
        previous: Listener<ActivateEvent>,
        next: Listener<ActivateEvent>,
    ) -> Self {
        Self {
            session,
            previous,
            next,
        }
    }
}

impl RenderOnce for ReaderScreen<'_> {
    fn render(self, _cx: &AppContext<'_>) -> impl IntoElement {
        div()
            .relative()
            .w_full()
            .h_full()
            .overflow_hidden()
            .child(ReaderPageView::new(
                self.session.current_page(),
                self.session.viewport(),
                self.session.resources(),
            ))
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
    }
}
