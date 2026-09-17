use inkpaper_reader::{
    FontWeight as ReaderFontWeight, ImageFragment, Page, PageItem, Rect as ReaderRect, TextFragment,
};
use inkpaper_ui::prelude::*;

use crate::ReaderDocument;

pub(crate) fn paint_reader_page(
    page: &Page<'static>,
    document: &ReaderDocument,
    paint: &mut PaintCx<'_>,
) {
    for item in page.items() {
        match item {
            PageItem::Text(fragment) => paint_text_fragment(fragment, paint),
            PageItem::Image(fragment) => paint_image_fragment(fragment, document, paint),
        }
    }
}

fn paint_text_fragment(fragment: &TextFragment<'_>, paint: &mut PaintCx<'_>) {
    let bounds = fragment.bounds();
    let style = fragment.style();

    paint.draw_text(
        reader_rect(bounds),
        text(fragment.text())
            .font_weight(reader_font_weight(style.font_weight()))
            .font_size(px(i32::from(style.font_size())))
            .line_height(reader_px(bounds.height()))
            .no_wrap(),
    );
}

fn paint_image_fragment(
    fragment: &ImageFragment<'_>,
    document: &ReaderDocument,
    paint: &mut PaintCx<'_>,
) {
    let Some(source) = document.image_source(fragment.image().path()) else {
        return;
    };

    paint.draw_image(
        reader_rect(fragment.bounds()),
        source,
        ImagePaint {
            fit: ImageFit::Fill,
            sampling: ImageSampling::Area,
            ..ImagePaint::DEFAULT
        },
    );
}

fn reader_rect(bounds: ReaderRect) -> Rect {
    Rect::new(
        Point::new(reader_px(bounds.x()), reader_px(bounds.y())),
        Size::new(reader_px(bounds.width()), reader_px(bounds.height())),
    )
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
