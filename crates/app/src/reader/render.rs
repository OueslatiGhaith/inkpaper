use inkpaper_reader::{
    ChapterImage, ImageDimensions, ImageFragment, Page, PageItem, Rect as ReaderRect, TextFragment,
    TextStyle as ReaderTextStyle, Viewport,
};
use inkpaper_ui::{
    AppContext, Div, Element, FontId, ImageSource, IntoElement, MountCx, MountError, NodeId,
    ParentElement, Pixels, RenderOnce, Size, Styled, TextStyled, div, image, px, text,
};

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

pub struct ReaderPageView<'page, 'chapter, R: ?Sized> {
    page: &'page Page<'chapter>,
    viewport: Viewport,
    resources: &'page R,
}

impl<'page, 'chapter, R> ReaderPageView<'page, 'chapter, R>
where
    R: ReaderPageResources + ?Sized,
{
    pub const fn new(page: &'page Page<'chapter>, viewport: Viewport, resources: &'page R) -> Self {
        Self {
            page,
            viewport,
            resources,
        }
    }
}

impl<'page, 'chapter, R> RenderOnce for ReaderPageView<'page, 'chapter, R>
where
    R: ReaderPageResources + ?Sized,
{
    fn render(self, _cx: &AppContext<'_>) -> impl IntoElement {
        let resources = self.resources;

        let items = self
            .page
            .items()
            .iter()
            .map(move |item| ReaderPageItem { item, resources });

        div()
            .relative()
            .w(reader_px(self.viewport.width()))
            .h(reader_px(self.viewport.height()))
            .overflow_hidden()
            .children(items)
    }
}

struct ReaderPageItem<'resource, 'chapter, R: ?Sized> {
    item: &'resource PageItem<'chapter>,
    resources: &'resource R,
}

impl<R> Element for ReaderPageItem<'_, '_, R>
where
    R: ReaderPageResources + ?Sized,
{
    fn mount(self, cx: &mut MountCx<'_>) -> Result<NodeId, MountError> {
        match self.item {
            PageItem::Text(fragment) => mount_text_fragment(fragment, self.resources, cx),
            PageItem::Image(fragment) => mount_image_fragment(fragment, self.resources, cx),
        }
    }
}

fn mount_text_fragment<R>(
    fragment: &TextFragment<'_>,
    resources: &R,
    cx: &mut MountCx<'_>,
) -> Result<NodeId, MountError>
where
    R: ReaderPageResources + ?Sized,
{
    let bounds = fragment.bounds();
    let style = fragment.style();
    let font = resources.font_for(style);

    positioned_box(bounds)
        .overflow_hidden()
        .child(
            text(fragment.text())
                .font(font)
                .font_size(px(i32::from(style.font_size())))
                .line_height(reader_px(bounds.height()))
                .no_wrap(),
        )
        .mount(cx)
}

fn mount_image_fragment<R>(
    fragment: &ImageFragment<'_>,
    resources: &R,
    cx: &mut MountCx<'_>,
) -> Result<NodeId, MountError>
where
    R: ReaderPageResources + ?Sized,
{
    let bounds = fragment.bounds();
    let container = positioned_box(bounds).overflow_hidden();

    let Some(source) = resources.image_source(fragment.image()) else {
        return container.mount(cx);
    };

    container
        .child(
            image(source)
                .size(Size::new(
                    reader_px(bounds.width()),
                    reader_px(bounds.height()),
                ))
                .fill(),
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
