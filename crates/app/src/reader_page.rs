use inkpaper_reader::{
    FontStyle as ReaderFontStyle, FontWeight as ReaderFontWeight, ImageFragment, Page, PageItem,
    Rect as ReaderRect, TextFragment,
};
use inkpaper_ui::prelude::*;

use crate::{ReaderDocument, typography::READER_FAMILY};

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
    let (font_weight, font_style) = reader_font(style.font_weight(), style.font_style());

    paint.draw_text_run(
        reader_rect(bounds),
        text(fragment.text())
            .font_family(READER_FAMILY)
            .font_weight(font_weight)
            .font_style(font_style)
            .font_size(px(i32::from(style.font_size())))
            .word_spacing(reader_px(fragment.word_spacing())),
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

/// the reader family's weight and style for a book's text style
pub(crate) fn reader_font(
    weight: ReaderFontWeight,
    style: ReaderFontStyle,
) -> (FontWeight, FontStyle) {
    let weight = match weight {
        ReaderFontWeight::Normal => FontWeight::NORMAL,
        ReaderFontWeight::Bold => FontWeight::BOLD,
    };

    let style = match style {
        ReaderFontStyle::Normal => FontStyle::Normal,
        ReaderFontStyle::Italic => FontStyle::Italic,
    };

    (weight, style)
}

fn reader_px(value: u32) -> Pixels {
    px(i32::try_from(value).unwrap_or(i32::MAX))
}
