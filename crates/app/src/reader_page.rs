use inkpaper_reader::{
    FontWeight as ReaderFontWeight, ImageFragment, Page, PageItem, Rect as ReaderRect,
    TextFragment, Viewport,
};
use inkpaper_ui::prelude::*;

use crate::ReaderDocument;

pub(crate) struct ReaderPageView<'a> {
    page: &'a Page<'static>,
    document: &'a ReaderDocument,
    viewport: Viewport,
}

impl<'a> ReaderPageView<'a> {
    pub(crate) const fn new(
        page: &'a Page<'static>,
        document: &'a ReaderDocument,
        viewport: Viewport,
    ) -> Self {
        Self {
            page,
            document,
            viewport,
        }
    }
}

impl RenderOnce for ReaderPageView<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let document = self.document;

        let items = self
            .page
            .items()
            .iter()
            .map(move |item| ReaderPageItem { item, document });

        div()
            .relative()
            .w(reader_px(self.viewport.width()))
            .h(reader_px(self.viewport.height()))
            .overflow_hidden()
            .children(items)
    }
}

struct ReaderPageItem<'a> {
    item: &'a PageItem<'static>,
    document: &'a ReaderDocument,
}

impl Element for ReaderPageItem<'_> {
    fn mount(self, cx: &mut MountCx<'_>) -> Result<NodeId, MountError> {
        match self.item {
            PageItem::Text(fragment) => mount_text_fragment(fragment, cx),
            PageItem::Image(fragment) => mount_image_fragment(fragment, self.document, cx),
        }
    }
}

fn mount_text_fragment(
    fragment: &TextFragment<'_>,
    cx: &mut MountCx<'_>,
) -> Result<NodeId, MountError> {
    let bounds = fragment.bounds();
    let style = fragment.style();

    positioned_box(bounds)
        .overflow_hidden()
        .child(
            text(fragment.text())
                .font_weight(reader_font_weight(style.font_weight()))
                .font_size(px(i32::from(style.font_size())))
                .line_height(reader_px(bounds.height()))
                .no_wrap(),
        )
        .mount(cx)
}

fn mount_image_fragment(
    fragment: &ImageFragment<'_>,
    document: &ReaderDocument,
    cx: &mut MountCx<'_>,
) -> Result<NodeId, MountError> {
    let bounds = fragment.bounds();

    let Some(source) = document.image_source(fragment.image().path()) else {
        return positioned_box(bounds).overflow_hidden().mount(cx);
    };

    positioned_box(bounds)
        .overflow_hidden()
        .child(
            image(source)
                .size(Size::new(
                    reader_px(bounds.width()),
                    reader_px(bounds.height()),
                ))
                .fill()
                .sampling(ImageSampling::Area),
        )
        .mount(cx)
}

fn positioned_box(bounds: ReaderRect) -> Div {
    div()
        .absolute()
        .left(reader_px(bounds.x()))
        .top(reader_px(bounds.y()))
        .w(reader_px(bounds.width()))
        .h(reader_px(bounds.height()))
}

fn reader_font_weight(weight: ReaderFontWeight) -> FontWeight {
    match weight {
        ReaderFontWeight::Normal => FontWeight::NORMAL,
        ReaderFontWeight::Bold => FontWeight::BOLD,
    }
}

fn reader_px(value: u32) -> Pixels {
    px(i32::try_from(value).unwrap_or(i32::MAX))
}
