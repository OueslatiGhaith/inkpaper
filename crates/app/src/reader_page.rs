use inkpaper_reader::{
    FontStyle as ReaderFontStyle, FontWeight as ReaderFontWeight, ImageFragment, Page, PageItem,
    Rect as ReaderRect, TextFragment,
};
use inkpaper_ui::{BoxPaint, prelude::*};

use crate::{ReaderDocument, reader::TextSettings};

/// A book's page, with the `selected` pieces of a word, each a text item's
/// index and the bounds of the word's highlight in it, inverted like
/// crosspoint's.
pub(crate) fn paint_reader_page(
    page: &Page<'static>,
    document: &ReaderDocument,
    family: FontFamilyId,
    settings: TextSettings,
    selected: &[(usize, ReaderRect)],
    paint: &mut PaintCx<'_>,
) {
    paint_page(page, Some(document), family, settings, paint);

    for &(index, bounds) in selected {
        let Some(PageItem::Text(fragment)) = page.items().get(index) else {
            continue;
        };

        let highlight = reader_rect(bounds);

        paint.draw_box(
            highlight,
            BoxPaint {
                background: Some(Color::BLACK),
                border: None,
                radius: px(3),
            },
        );

        // the item's text again, in white, showing only the word
        paint.with_clip(highlight, |paint| {
            paint_text_fragment(fragment, family, settings, Some(Color::WHITE), paint);
        });
    }
}

/// A laid out page, its text in `family`, the one it was laid out in, and
/// painted as `settings` say. Its images come from `document`, so a page
/// without one, like a settings preview, draws only its text.
pub(crate) fn paint_page(
    page: &Page<'static>,
    document: Option<&ReaderDocument>,
    family: FontFamilyId,
    settings: TextSettings,
    paint: &mut PaintCx<'_>,
) {
    for item in page.items() {
        match item {
            PageItem::Text(fragment) => {
                paint_text_fragment(fragment, family, settings, None, paint)
            }
            PageItem::Image(fragment) => {
                if let Some(document) = document {
                    paint_image_fragment(fragment, document, paint);
                }
            }
        }
    }
}

/// `fragment`'s text, painted as `settings` say, in `color` when given.
fn paint_text_fragment(
    fragment: &TextFragment<'_>,
    family: FontFamilyId,
    settings: TextSettings,
    color: Option<Color>,
    paint: &mut PaintCx<'_>,
) {
    let bounds = fragment.bounds();
    let style = fragment.style();
    let (font_weight, font_style) = reader_font(style.font_weight(), style.font_style());

    let run = text(fragment.text())
        .font_family(family)
        .font_weight(font_weight)
        .font_style(font_style)
        .font_size(px(i32::from(style.font_size())))
        .word_spacing(reader_px(fragment.word_spacing()))
        .text_darkness(settings.darkness().ink())
        .text_antialias(settings.antialiasing());

    // like crosspoint, links into the book are underlined; the device can't
    // open the web, so external ones aren't
    let run = if fragment.link().is_some_and(|link| !link.is_external()) {
        run.underline()
    } else {
        run
    };

    let run = match color {
        Some(color) => run.text_color(color),
        None => run,
    };

    paint.draw_text_run(reader_rect(bounds), run);
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
