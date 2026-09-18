use crate::{Color, ImageResource, ImageSampling};

mod area;
mod bilinear;
mod nearest;

#[derive(Clone, Copy)]
pub(crate) struct ImageSampler<'image> {
    image: &'image dyn ImageResource,
    source_width: u32,
    source_height: u32,
    destination_width: u32,
    destination_height: u32,
    sampling: ImageSampling,
}

#[derive(Clone, Copy)]
pub(crate) struct ImageSamplingRow<'image> {
    image: &'image dyn ImageResource,
    source_width: u32,
    destination_width: u32,
    kind: ImageSamplingRowKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ImageSamplingRowKind {
    Nearest(nearest::NearestRow),
    Bilinear(bilinear::BilinearRow),
    Area(area::AreaRow),
}

impl<'image> ImageSampler<'image> {
    pub(crate) fn new(
        image: &'image dyn ImageResource,
        source_width: u32,
        source_height: u32,
        destination_width: u32,
        destination_height: u32,
        sampling: ImageSampling,
    ) -> Option<Self> {
        if source_width == 0
            || source_height == 0
            || destination_width == 0
            || destination_height == 0
        {
            return None;
        }

        // exact area filtering only applies when both axes are being preserved or minified.
        // Preserve the existing bilinear fallback for magnification, but decide it
        // once for the whole image instead of once per pixel.
        let sampling = match sampling {
            ImageSampling::Area
                if destination_width > source_width || destination_height > source_height =>
            {
                ImageSampling::Bilinear
            }

            sampling => sampling,
        };

        Some(Self {
            image,
            source_width,
            source_height,
            destination_width,
            destination_height,
            sampling,
        })
    }

    pub(crate) fn row(self, y: u32) -> Option<ImageSamplingRow<'image>> {
        if y >= self.destination_height {
            return None;
        }

        let kind = match self.sampling {
            ImageSampling::Nearest => ImageSamplingRowKind::Nearest(nearest::prepare_row(
                y,
                self.source_height,
                self.destination_height,
            )?),

            ImageSampling::Bilinear => ImageSamplingRowKind::Bilinear(bilinear::prepare_row(
                y,
                self.source_height,
                self.destination_height,
            )?),

            ImageSampling::Area => ImageSamplingRowKind::Area(area::prepare_row(
                y,
                self.source_width,
                self.source_height,
                self.destination_height,
            )?),
        };

        Some(ImageSamplingRow {
            image: self.image,
            source_width: self.source_width,
            destination_width: self.destination_width,
            kind,
        })
    }
}

impl ImageSamplingRow<'_> {
    pub(crate) fn sample(self, x: u32) -> Option<Color> {
        if x >= self.destination_width {
            return None;
        }

        match self.kind {
            ImageSamplingRowKind::Nearest(row) => nearest::sample_nearest(
                self.image,
                x,
                self.source_width,
                self.destination_width,
                row,
            ),

            ImageSamplingRowKind::Bilinear(row) => bilinear::sample_bilinear(
                self.image,
                x,
                self.source_width,
                self.destination_width,
                row,
            ),

            ImageSamplingRowKind::Area(row) => area::sample_area(
                self.image,
                x,
                self.source_width,
                self.destination_width,
                row,
            ),
        }
    }
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
fn sample_image(
    image: &dyn ImageResource,
    x: u32,
    y: u32,
    source_width: u32,
    source_height: u32,
    destination_width: u32,
    destination_height: u32,
    sampling: ImageSampling,
) -> Option<Color> {
    let sampler = ImageSampler::new(
        image,
        source_width,
        source_height,
        destination_width,
        destination_height,
        sampling,
    )?;

    sampler.row(y)?.sample(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::{Size, px};

    struct WhiteCornerImage;

    impl ImageResource for WhiteCornerImage {
        fn size(&self) -> Size {
            Size::new(px(4), px(4))
        }

        fn pixel(&self, x: u32, y: u32) -> Option<Color> {
            if x >= 4 || y >= 4 {
                return None;
            }

            if x == 0 && y == 0 {
                Some(Color::WHITE)
            } else {
                Some(Color::BLACK)
            }
        }
    }

    struct ThreePixelStrip;

    impl ImageResource for ThreePixelStrip {
        fn size(&self) -> Size {
            Size::new(px(3), px(1))
        }

        fn pixel(&self, x: u32, y: u32) -> Option<Color> {
            if y != 0 {
                return None;
            }

            match x {
                0 | 2 => Some(Color::BLACK),
                1 => Some(Color::WHITE),
                _ => None,
            }
        }
    }

    struct FourColorImage;

    impl ImageResource for FourColorImage {
        fn size(&self) -> Size {
            Size::new(px(2), px(2))
        }

        fn pixel(&self, x: u32, y: u32) -> Option<Color> {
            match (x, y) {
                (0, 0) => Some(Color::BLACK),
                (1, 0) => Some(Color::RED),
                (0, 1) => Some(Color::GREEN),
                (1, 1) => Some(Color::BLUE),
                _ => None,
            }
        }
    }

    #[test]
    fn area_sampling_averages_entire_source_footprint() {
        let image = WhiteCornerImage;

        let color = sample_image(&image, 0, 0, 4, 4, 1, 1, ImageSampling::Area).unwrap();

        // one of sixteen source pixels is white.
        // 255 / 16 = 15.9375,  rounded to 16.
        assert_eq!(color, Color::rgb(16, 16, 16));
    }

    #[test]
    fn area_sampling_weights_partial_source_pixels() {
        let image = ThreePixelStrip;

        let left = sample_image(&image, 0, 0, 3, 1, 2, 1, ImageSampling::Area).unwrap();
        let right = sample_image(&image, 1, 0, 3, 1, 2, 1, ImageSampling::Area).unwrap();

        // destination 0 covers:
        //   source 0: weight 2
        //   source 1: weight 1
        // destination 1 covers:
        //   source 1: weight 1
        //   source 2: weight 2
        // therefore both produce:
        // 255 / 3 = 85.
        let expected = Color::rgb(85, 85, 85);

        assert_eq!(left, expected);
        assert_eq!(right, expected);
    }

    #[test]
    fn area_sampling_preserves_native_pixels() {
        let image = ThreePixelStrip;

        assert_eq!(
            sample_image(&image, 0, 0, 3, 1, 3, 1, ImageSampling::Area),
            Some(Color::BLACK),
        );
        assert_eq!(
            sample_image(&image, 1, 0, 3, 1, 3, 1, ImageSampling::Area),
            Some(Color::WHITE),
        );
        assert_eq!(
            sample_image(&image, 2, 0, 3, 1, 3, 1, ImageSampling::Area),
            Some(Color::BLACK),
        );
    }

    #[test]
    fn area_sampling_falls_back_to_bilinear_when_magnifying() {
        let image = FourColorImage;

        for y in 0..3 {
            for x in 0..3 {
                let area = sample_image(&image, x, y, 2, 2, 3, 3, ImageSampling::Area);

                let bilinear = sample_image(&image, x, y, 2, 2, 3, 3, ImageSampling::Bilinear);

                assert_eq!(area, bilinear, "area fallback differs at ({x}, {y})");
            }
        }
    }
}
