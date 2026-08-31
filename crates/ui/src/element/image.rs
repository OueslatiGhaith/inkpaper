use crate::{
    Element, MountCx, MountError, NodeId, Pixels, Point, Rect, Size, image::ImageSource, px,
};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ImageFit {
    #[default]
    None,
    Contain,
    Cover,
    Fill,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ImagePosition {
    #[default]
    Center,
    Top,
    Bottom,
    Left,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ImageSampling {
    #[default]
    Nearest,
    Bilinear,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct ImagePaint {
    pub fit: ImageFit,
    pub position: ImagePosition,
    pub sampling: ImageSampling,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub(crate) struct ImageStyle {
    pub(crate) width: Option<Pixels>,
    pub(crate) height: Option<Pixels>,
    pub(crate) paint: ImagePaint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
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
                paint: ImagePaint {
                    fit: ImageFit::None,
                    position: ImagePosition::Center,
                    sampling: ImageSampling::Nearest,
                },
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
        self.style.paint.fit = fit;
        self
    }

    pub fn position(mut self, position: ImagePosition) -> Self {
        self.style.paint.position = position;
        self
    }

    pub fn sampling(mut self, sampling: ImageSampling) -> Self {
        self.style.paint.sampling = sampling;
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

fn positioned_image_bounds(bounds: Rect, size: Size, position: ImagePosition) -> Rect {
    use ImagePosition::*;

    let remaining_x = bounds.width() - size.width;
    let remaining_y = bounds.height() - size.height;

    let x = match position {
        Left | TopLeft | BottomLeft => bounds.origin.x,
        Right | TopRight | BottomRight => bounds.origin.x + remaining_x,
        Center | Top | Bottom => bounds.origin.x + remaining_x / 2,
    };

    let y = match position {
        Top | TopLeft | TopRight => bounds.origin.y,
        Bottom | BottomLeft | BottomRight => bounds.origin.y + remaining_y,
        Center | Left | Right => bounds.origin.y + remaining_y / 2,
    };

    Rect::new(Point::new(x, y), size)
}

pub(crate) fn fitted_image_bounds(
    source_size: Size,
    bounds: Rect,
    fit: ImageFit,
    position: ImagePosition,
) -> Rect {
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

    let size = match fit {
        ImageFit::None => Size::new(source_width, source_height),
        ImageFit::Fill => {
            return Rect::new(bounds.origin, Size::new(bounds_width, bounds_height));
        }
        ImageFit::Contain => {
            let height_at_full_width = source_height.scale_ratio_floor(bounds_width, source_width);

            if height_at_full_width <= bounds_height {
                Size::new(bounds_width, height_at_full_width.max(px(1)))
            } else {
                let width = source_width.scale_ratio_floor(bounds_height, source_height);

                Size::new(width.max(px(1)), bounds_height)
            }
        }

        ImageFit::Cover => {
            let height_at_full_width = source_height.scale_ratio_ceil(bounds_width, source_width);
            if height_at_full_width >= bounds_height {
                Size::new(bounds_width, height_at_full_width.max(px(1)))
            } else {
                let width = source_width.scale_ratio_ceil(bounds_height, source_height);
                Size::new(width.max(px(1)), bounds_height)
            }
        }
    };

    positioned_image_bounds(bounds, size, position)
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
            ImagePosition::Center,
        );

        assert_eq!(
            fitted,
            Rect::new(Point::new(px(0), px(2)), Size::new(px(8), px(4)))
        );
    }

    #[test]
    fn cover_preserves_aspect_ratio_and_crops_center() {
        let fitted = fitted_image_bounds(
            Size::new(px(4), px(2)),
            Rect::new(Point::ZERO, Size::new(px(8), px(8))),
            ImageFit::Cover,
            ImagePosition::Center,
        );

        assert_eq!(
            fitted,
            Rect::new(Point::new(px(-4), px(0)), Size::new(px(16), px(8)))
        );
    }

    #[test]
    fn cover_can_align_horizontal_crop_to_edges() {
        let bounds = Rect::new(Point::ZERO, Size::new(px(8), px(8)));
        let source = Size::new(px(4), px(2));

        assert_eq!(
            fitted_image_bounds(source, bounds, ImageFit::Cover, ImagePosition::Left),
            Rect::new(Point::ZERO, Size::new(px(16), px(8)))
        );

        assert_eq!(
            fitted_image_bounds(source, bounds, ImageFit::Cover, ImagePosition::Right),
            Rect::new(Point::new(px(-8), px(0)), Size::new(px(16), px(8)))
        );
    }

    #[test]
    fn cover_can_align_vertical_crop_to_edges() {
        let bounds = Rect::new(Point::ZERO, Size::new(px(8), px(8)));
        let source = Size::new(px(2), px(4));

        assert_eq!(
            fitted_image_bounds(source, bounds, ImageFit::Cover, ImagePosition::Top),
            Rect::new(Point::ZERO, Size::new(px(8), px(16)))
        );
        assert_eq!(
            fitted_image_bounds(source, bounds, ImageFit::Cover, ImagePosition::Bottom),
            Rect::new(Point::new(px(0), px(-8)), Size::new(px(8), px(16)))
        );
    }

    #[test]
    fn contain_can_align_image_inside_unused_space() {
        let fitted = fitted_image_bounds(
            Size::new(px(4), px(2)),
            Rect::new(Point::ZERO, Size::new(px(8), px(8))),
            ImageFit::Contain,
            ImagePosition::Bottom,
        );

        assert_eq!(
            fitted,
            Rect::new(Point::new(px(0), px(4)), Size::new(px(8), px(4)))
        );
    }

    #[test]
    fn fill_uses_exact_layout_bounds_regardless_of_position() {
        let bounds = Rect::new(Point::new(px(3), px(7)), Size::new(px(25), px(40)));

        assert_eq!(
            fitted_image_bounds(
                Size::new(px(10), px(5)),
                bounds,
                ImageFit::Fill,
                ImagePosition::BottomRight,
            ),
            bounds
        );
    }

    #[test]
    fn native_fit_can_be_explicitly_aligned() {
        let fitted = fitted_image_bounds(
            Size::new(px(20), px(10)),
            Rect::new(Point::new(px(5), px(7)), Size::new(px(100), px(100))),
            ImageFit::None,
            ImagePosition::TopLeft,
        );

        assert_eq!(
            fitted,
            Rect::new(Point::new(px(5), px(7)), Size::new(px(20), px(10)))
        );
    }
}
