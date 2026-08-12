use crate::{Element, MountCx, MountError, NodeId, Pixels, Point, Rect, Size, px};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ImageId(u16);

impl ImageId {
    pub const fn new(value: u16) -> Self {
        Self(value)
    }

    pub(crate) const fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageSource {
    id: ImageId,
    size: Size,
}

impl ImageSource {
    pub const fn new(id: ImageId, size: Size) -> Self {
        Self { id, size }
    }

    pub const fn id(self) -> ImageId {
        self.id
    }

    pub const fn size(self) -> Size {
        self.size
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ImageFit {
    #[default]
    None,
    Contain,
    Cover,
    Fill,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ImageStyle {
    pub(crate) width: Option<Pixels>,
    pub(crate) height: Option<Pixels>,
    pub(crate) fit: ImageFit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Image {
    source: ImageSource,
    style: ImageStyle,
}

impl Image {
    pub const fn new(source: ImageSource) -> Self {
        Self {
            source,
            style: ImageStyle {
                width: None,
                height: None,
                fit: ImageFit::None,
            },
        }
    }

    pub const fn source(self) -> ImageSource {
        self.source
    }

    pub fn w(mut self, width: Pixels) -> Self {
        self.style.width = Some(width);
        self
    }

    pub fn h(mut self, height: Pixels) -> Self {
        self.style.height = Some(height);
        self
    }

    pub fn size(mut self, size: Size) -> Self {
        self.style.width = Some(size.width);
        self.style.height = Some(size.height);
        self
    }

    pub fn fit(mut self, fit: ImageFit) -> Self {
        self.style.fit = fit;
        self
    }

    pub fn contain(self) -> Self {
        self.fit(ImageFit::Contain)
    }

    pub fn cover(self) -> Self {
        self.fit(ImageFit::Cover)
    }

    pub fn fill(self) -> Self {
        self.fit(ImageFit::Fill)
    }

    pub fn native(self) -> Self {
        self.fit(ImageFit::None)
    }
}

impl Element for Image {
    fn mount(self, cx: &mut MountCx<'_>) -> Result<NodeId, MountError> {
        cx.push_image(self.source, self.style)
    }
}

pub fn image(source: impl Into<ImageSource>) -> Image {
    Image::new(source.into())
}

fn centered_image_bounds(bounds: Rect, size: Size) -> Rect {
    let x = bounds.origin.x + (bounds.width() - size.width) / 2;
    let y = bounds.origin.y + (bounds.height() - size.height) / 2;

    Rect::new(Point::new(x, y), size)
}

pub(crate) fn fitted_image_bounds(source_size: Size, bounds: Rect, fit: ImageFit) -> Rect {
    let source_width = source_size.width.non_negative();
    let source_height = source_size.height.non_negative();
    let bounds_width = bounds.width().non_negative();
    let bounds_height = bounds.height().non_negative();

    if source_width.is_non_positive()
        || source_height.is_non_positive()
        || bounds_width.is_non_positive()
        || bounds_height.is_non_positive()
    {
        return Rect::new(bounds.origin, Size::ZERO);
    }

    match fit {
        ImageFit::None => Rect::new(bounds.origin, Size::new(source_width, source_height)),
        ImageFit::Fill => Rect::new(bounds.origin, Size::new(bounds_width, bounds_height)),
        ImageFit::Contain => {
            let height_at_full_width = source_height.scale_ratio_floor(bounds_width, source_width);
            let size = if height_at_full_width <= bounds_height {
                Size::new(bounds_width, height_at_full_width.max(px(1)))
            } else {
                let width = source_width.scale_ratio_floor(bounds_height, source_height);
                Size::new(width.max(px(1)), bounds_height)
            };

            centered_image_bounds(bounds, size)
        }
        ImageFit::Cover => {
            let height_at_full_width = source_height.scale_ratio_ceil(bounds_width, source_width);
            let size = if height_at_full_width >= bounds_height {
                Size::new(bounds_width, height_at_full_width.max(px(1)))
            } else {
                let width = source_width.scale_ratio_ceil(bounds_height, source_height);
                Size::new(width.max(px(1)), bounds_height)
            };

            centered_image_bounds(bounds, size)
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::*;

    #[test]
    fn contain_preserves_aspect_ratio_and_centers_image() {
        let fitted = fitted_image_bounds(
            Size::new(px(4), px(2)),
            Rect::new(Point::ZERO, Size::new(px(8), px(8))),
            ImageFit::Contain,
        );

        assert_eq!(
            fitted,
            Rect::new(Point::new(px(0), px(2),), Size::new(px(8), px(4),),)
        );
    }

    #[test]
    fn cover_preserves_aspect_ratio_and_crops_center() {
        let fitted = fitted_image_bounds(
            Size::new(px(4), px(2)),
            Rect::new(Point::ZERO, Size::new(px(8), px(8))),
            ImageFit::Cover,
        );

        assert_eq!(
            fitted,
            Rect::new(Point::new(px(-4), px(0),), Size::new(px(16), px(8),),)
        );
    }

    #[test]
    fn fill_uses_exact_layout_bounds() {
        let bounds = Rect::new(Point::new(px(3), px(7)), Size::new(px(25), px(40)));

        assert_eq!(
            fitted_image_bounds(Size::new(px(10), px(5),), bounds, ImageFit::Fill,),
            bounds
        );
    }

    #[test]
    fn native_fit_keeps_intrinsic_image_size() {
        let fitted = fitted_image_bounds(
            Size::new(px(20), px(10)),
            Rect::new(Point::new(px(5), px(7)), Size::new(px(100), px(100))),
            ImageFit::None,
        );

        assert_eq!(
            fitted,
            Rect::new(Point::new(px(5), px(7),), Size::new(px(20), px(10),),)
        );
    }
}
