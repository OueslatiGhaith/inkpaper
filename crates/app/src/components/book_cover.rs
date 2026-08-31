use inkpaper_ui::prelude::*;

use crate::theme::Theme;

pub struct BookCover {
    source: Option<ImageSource>,
    size: Size,
}

impl BookCover {
    pub const fn new(source: Option<ImageSource>, size: Size) -> Self {
        Self { source, size }
    }
}

impl RenderOnce for BookCover {
    fn render(self, cx: &AppContext<'_>) -> impl IntoElement {
        let theme = cx.global::<Theme>();

        match self.source {
            Some(source) => Either::Left(
                image(source)
                    .size(self.size)
                    .cover()
                    .sampling(ImageSampling::Bilinear)
                    .monochrome()
                    .dither(ImageDither::Bayer4x4),
            ),

            None => Either::Right(
                div()
                    .w(self.size.width)
                    .h(self.size.height)
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(theme.ink)
                    .text_color(theme.paper)
                    .child("BOOK"),
            ),
        }
    }
}
