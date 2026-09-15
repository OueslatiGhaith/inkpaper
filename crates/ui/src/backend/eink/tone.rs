use embedded_graphics::{
    Pixel,
    geometry::Point,
    pixelcolor::{Gray2, GrayColor},
    prelude::{Dimensions, DrawTarget},
    primitives::{PointsIter, Rectangle},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EInkUiMode {
    #[default]
    NativeGray2,
    BinaryDither,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum EInkTone {
    #[default]
    Binary,
    Gray4,
}

impl EInkTone {
    pub const fn has_native_gray(self) -> bool {
        matches!(self, Self::Gray4)
    }

    pub(super) const fn merged(self, other: Self) -> Self {
        if self.has_native_gray() || other.has_native_gray() {
            Self::Gray4
        } else {
            Self::Binary
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct EInkPaintReport {
    tone: EInkTone,
}

impl EInkPaintReport {
    pub const fn tone(self) -> EInkTone {
        self.tone
    }

    pub const fn has_native_gray(self) -> bool {
        self.tone.has_native_gray()
    }

    pub(super) fn include(&mut self, tone: EInkTone) {
        self.tone = self.tone.merged(tone);
    }
}

pub(super) fn gray2_tone(color: Gray2) -> EInkTone {
    match color.luma() {
        0 | 3 => EInkTone::Binary,
        _ => EInkTone::Gray4,
    }
}

pub(super) struct BinaryDitherTarget<'a, D> {
    target: &'a mut D,
}

impl<'a, D> BinaryDitherTarget<'a, D> {
    pub(super) fn new(target: &'a mut D) -> Self {
        Self { target }
    }
}

impl<D> Dimensions for BinaryDitherTarget<'_, D>
where
    D: DrawTarget<Color = Gray2>,
{
    fn bounding_box(&self) -> Rectangle {
        self.target.bounding_box()
    }
}

impl<D> DrawTarget for BinaryDitherTarget<'_, D>
where
    D: DrawTarget<Color = Gray2>,
{
    type Color = Gray2;
    type Error = D::Error;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        self.target.draw_iter(
            pixels
                .into_iter()
                .map(|Pixel(point, color)| Pixel(point, binary_dither_gray2(color, point))),
        )
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        // Preserve the underlying target's optimized fill for actual B/W.
        if color.luma() == 0 || color.luma() == 3 {
            return self.target.fill_solid(area, color);
        }

        self.draw_iter(area.points().map(|point| Pixel(point, color)))
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        self.fill_solid(&self.bounding_box(), color)
    }
}

pub(super) fn binary_dither_gray2(color: Gray2, point: Point) -> Gray2 {
    match color.luma() {
        0 => Gray2::new(0),
        3 => Gray2::new(3),
        level => {
            let luminance = u16::from(level) * 85;
            let threshold = u16::from(bayer_threshold(point));

            if luminance > threshold {
                Gray2::new(3)
            } else {
                Gray2::new(0)
            }
        }
    }
}

pub(super) fn bayer_threshold(point: Point) -> u8 {
    const BAYER_4X4: [u8; 16] = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5];

    let x = point.x.rem_euclid(4) as usize;
    let y = point.y.rem_euclid(4) as usize;
    let rank = BAYER_4X4[y * 4 + x];

    rank.saturating_mul(16).saturating_add(8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_dither_never_outputs_native_gray() {
        for y in 0..4 {
            for x in 0..4 {
                for level in 0..=3 {
                    let output = binary_dither_gray2(Gray2::new(level), Point::new(x, y));

                    assert!(output.luma() == 0 || output.luma() == 3);
                }
            }
        }
    }

    #[test]
    fn black_and_white_are_not_dithered() {
        for y in 0..4 {
            for x in 0..4 {
                let point = Point::new(x, y);

                assert_eq!(binary_dither_gray2(Gray2::new(0), point), Gray2::new(0),);

                assert_eq!(binary_dither_gray2(Gray2::new(3), point), Gray2::new(3),);
            }
        }
    }

    #[test]
    fn middle_gray_produces_both_binary_levels() {
        let mut black = 0;
        let mut white = 0;

        for y in 0..4 {
            for x in 0..4 {
                match binary_dither_gray2(Gray2::new(2), Point::new(x, y)).luma() {
                    0 => black += 1,
                    3 => white += 1,
                    _ => unreachable!(),
                }
            }
        }

        assert!(black > 0);
        assert!(white > 0);
    }

    #[test]
    fn eink_paint_report_promotes_to_gray4() {
        let mut report = EInkPaintReport::default();

        assert_eq!(report.tone(), EInkTone::Binary);

        report.include(EInkTone::Binary);
        assert_eq!(report.tone(), EInkTone::Binary);

        report.include(EInkTone::Gray4);
        assert_eq!(report.tone(), EInkTone::Gray4);

        report.include(EInkTone::Binary);
        assert_eq!(report.tone(), EInkTone::Gray4);
    }

    #[test]
    fn gray2_tone_distinguishes_binary_and_native_gray() {
        assert_eq!(gray2_tone(Gray2::new(0)), EInkTone::Binary);
        assert_eq!(gray2_tone(Gray2::new(1)), EInkTone::Gray4);
        assert_eq!(gray2_tone(Gray2::new(2)), EInkTone::Gray4);
        assert_eq!(gray2_tone(Gray2::new(3)), EInkTone::Binary);
    }
}
