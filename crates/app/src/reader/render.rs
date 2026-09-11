use inkpaper_reader::{
    ChapterImage, ImageDimensions, Page, PageItem, Rect as ReaderRect,
    TextStyle as ReaderTextStyle, Viewport,
};
use inkpaper_ui::prelude::*;

pub trait ReaderPageResources {
    fn font_for(&self, _style: ReaderTextStyle) -> FontId {
        FontId::DEFAULT
    }

    fn image_dimensions(&self, _image: &ChapterImage) -> Option<ImageDimensions> {
        None
    }

    fn image_source(&self, _image: &ChapterImage) -> Option<ImageSource> {
        None
    }
}

impl ReaderPageResources for () {}

pub fn paint_reader_page<R: ReaderPageResources + ?Sized>(
    page: &Page<'_>,
    viewport: Viewport,
    resources: &R,
    cx: &mut PaintCx<'_>,
) {
    assert_eq!(
        cx.bounds().size,
        Size::new(reader_px(viewport.width()), reader_px(viewport.height())),
        "reader viewport changed, repaginate before painting"
    );

    for item in page.items() {
        match item {
            PageItem::Text(fragment) => {
                let bounds = fragment.bounds();
                let style = fragment.style();

                cx.draw_text(
                    ui_rect(bounds),
                    text(fragment.text())
                        .font(resources.font_for(style))
                        .font_size(px(i32::from(style.font_size())))
                        .line_height(reader_px(bounds.height()))
                        .no_wrap(),
                );
            }
            PageItem::Image(fragment) => {
                if let Some(source) = resources.image_source(fragment.image()) {
                    cx.draw_image(
                        ui_rect(fragment.bounds()),
                        source,
                        ImagePaint {
                            fit: ImageFit::Fill,
                            ..Default::default()
                        },
                    );
                }
            }
        }
    }
}

fn ui_rect(bounds: ReaderRect) -> Rect {
    Rect::new(
        Point::new(reader_px(bounds.x()), reader_px(bounds.y())),
        Size::new(reader_px(bounds.width()), reader_px(bounds.height())),
    )
}

fn reader_px(value: u32) -> Pixels {
    px(i32::try_from(value).unwrap_or(i32::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reader_pixels_convert_without_changing_normal_values() {
        assert_eq!(reader_px(0), px(0));
        assert_eq!(reader_px(42), px(42));
        assert_eq!(reader_px(i32::MAX as u32), px(i32::MAX));
    }

    #[test]
    fn reader_pixels_saturate_at_ui_pixel_limit() {
        assert_eq!(reader_px(u32::MAX), px(i32::MAX));
    }
}
