use embedded_graphics::{
    Pixel,
    draw_target::DrawTarget as EgDrawTarget,
    geometry::{OriginDimensions, Point as EgPoint, Size as EgSize},
    image::{GetPixel as EgGetPixel, ImageDrawable as EgImageDrawable},
    mock_display::MockDisplay,
    mono_font::ascii::FONT_6X10,
    pixelcolor::Rgb888,
    primitives::Rectangle as EgRectangle,
};

use super::{EmbeddedGraphicsImage, text::aligned_line_x};
use crate::{
    backend::{
        EmbeddedGraphicsPainter, MonoFontFace,
        embedded_graphics::coverage::{alpha_blend_rgb888, ordered_dither_accepts},
    },
    *,
};

static TEST_FONT_FACE: MonoFontFace<'static> = MonoFontFace::new(&FONT_6X10);

fn test_font_resources(storage: &mut [u8]) -> FontResources<'static, '_, 1, 64> {
    let mut resources = FontResources::new(storage);

    let id = resources.register(&TEST_FONT_FACE).unwrap();
    assert_eq!(id, FontId::DEFAULT,);

    resources
}

#[test]
fn embedded_graphics_backend_rasterizes_box() {
    let mut display = MockDisplay::<Rgb888>::new();

    {
        let mut glyph_storage = [0; 4096];
        let mut fonts = test_font_resources(&mut glyph_storage);
        let mut painter =
            EmbeddedGraphicsPainter::new(&mut display, &mut fonts, ImageRegistry::<0>::default());

        painter
            .draw_box(
                Rect::new(Point::new(px(0), px(0)), Size::new(px(10), px(10))),
                BoxPaint {
                    background: Some(Color::BLUE),
                    border: Some(BorderPaint {
                        width: px(1),
                        color: Color::RED,
                    }),
                    radius: px(0),
                },
                None,
            )
            .unwrap();
    }

    assert_eq!(
        display.get_pixel(EgPoint::new(0, 0,)),
        Some(Rgb888::new(255, 0, 0,))
    );
    assert_eq!(
        display.get_pixel(EgPoint::new(5, 5,)),
        Some(Rgb888::new(0, 0, 255,))
    );
}

#[test]
fn text_measurement_wraps_at_word_boundaries() {
    let mut display = MockDisplay::<Rgb888>::new();

    let mut glyph_storage = [0; 4096];
    let mut fonts = test_font_resources(&mut glyph_storage);
    let painter =
        EmbeddedGraphicsPainter::new(&mut display, &mut fonts, ImageRegistry::<0>::default());

    let size = painter.measure_text(
        "hello world",
        ResolvedTextStyle {
            wrap: TextWrap::Word,
            ..ResolvedTextStyle::default()
        },
        Size::new(px(30), px(100)),
    );

    assert_eq!(size, Size::new(px(30), px(20),));
}

#[test]
fn custom_line_height_affects_multiline_measurement() {
    let mut display = MockDisplay::<Rgb888>::new();

    let mut glyph_storage = [0; 4096];
    let mut fonts = test_font_resources(&mut glyph_storage);
    let painter =
        EmbeddedGraphicsPainter::new(&mut display, &mut fonts, ImageRegistry::<0>::default());

    let size = painter.measure_text(
        "first\nsecond",
        ResolvedTextStyle {
            line_height: LineHeight::Pixels(px(15)),
            ..ResolvedTextStyle::default()
        },
        Size::new(px(100), px(100)),
    );

    assert_eq!(size.height, px(25));
}

#[test]
fn centered_text_line_is_offset_inside_bounds() {
    let bounds = Rect::new(Point::new(px(10), px(5)), Size::new(px(100), px(20)));

    assert_eq!(
        aligned_line_x(
            bounds,
            px(40),
            TextAlign::Center,
            TextDirection::LeftToRight
        ),
        px(40)
    );
    assert_eq!(
        aligned_line_x(bounds, px(40), TextAlign::End, TextDirection::LeftToRight),
        px(70)
    );
    assert_eq!(
        aligned_line_x(bounds, px(40), TextAlign::Start, TextDirection::LeftToRight),
        px(10)
    );
}

#[test]
fn max_lines_limits_measured_height() {
    let mut display = MockDisplay::<Rgb888>::new();

    let mut glyph_storage = [0; 4096];
    let mut fonts = test_font_resources(&mut glyph_storage);
    let painter =
        EmbeddedGraphicsPainter::new(&mut display, &mut fonts, ImageRegistry::<0>::default());

    let size = painter.measure_text(
        "hello world again",
        ResolvedTextStyle {
            wrap: TextWrap::Word,
            max_lines: TextMaxLines::Limited(2),
            ..ResolvedTextStyle::default()
        },
        Size::new(px(30), px(100)),
    );

    assert_eq!(size.height, px(20));
}

#[test]
fn ellipsis_respects_available_width() {
    let mut display = MockDisplay::<Rgb888>::new();

    let mut glyph_storage = [0; 4096];
    let mut fonts = test_font_resources(&mut glyph_storage);
    let painter =
        EmbeddedGraphicsPainter::new(&mut display, &mut fonts, ImageRegistry::<0>::default());

    let size = painter.measure_text(
        "abcdefghij",
        ResolvedTextStyle {
            overflow: TextOverflow::Ellipsis,
            ..ResolvedTextStyle::default()
        },
        Size::new(px(30), px(100)),
    );

    assert_eq!(size, Size::new(px(30), px(10),));
}

#[test]
fn wrapped_text_can_be_clamped_with_ellipsis() {
    let mut display = MockDisplay::<Rgb888>::new();

    let mut glyph_storage = [0; 4096];
    let mut fonts = test_font_resources(&mut glyph_storage);
    let painter =
        EmbeddedGraphicsPainter::new(&mut display, &mut fonts, ImageRegistry::<0>::default());

    let size = painter.measure_text(
        "hello world again",
        ResolvedTextStyle {
            wrap: TextWrap::Word,
            max_lines: TextMaxLines::Limited(1),
            overflow: TextOverflow::Ellipsis,
            ..ResolvedTextStyle::default()
        },
        Size::new(px(30), px(100)),
    );

    assert_eq!(size, Size::new(px(30), px(10),));
}

struct SolidTestImage {
    size: EgSize,
    color: Rgb888,
}

impl OriginDimensions for SolidTestImage {
    fn size(&self) -> EgSize {
        self.size
    }
}

impl EgImageDrawable for SolidTestImage {
    type Color = Rgb888;

    fn draw<D>(&self, target: &mut D) -> Result<(), D::Error>
    where
        D: EgDrawTarget<Color = Self::Color>,
    {
        target.fill_solid(&EgRectangle::new(EgPoint::zero(), self.size), self.color)
    }

    fn draw_sub_image<D>(&self, target: &mut D, area: &EgRectangle) -> Result<(), D::Error>
    where
        D: EgDrawTarget<Color = Self::Color>,
    {
        target.fill_solid(area, self.color)
    }
}

impl EgGetPixel for SolidTestImage {
    type Color = Rgb888;

    fn pixel(&self, point: EgPoint) -> Option<Self::Color> {
        if point.x < 0 || point.y < 0 {
            return None;
        }

        let x = u32::try_from(point.x).ok()?;
        let y = u32::try_from(point.y).ok()?;
        if x >= self.size.width || y >= self.size.height {
            return None;
        }

        Some(self.color)
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

struct HorizontalStripImage;

impl ImageResource for HorizontalStripImage {
    fn size(&self) -> Size {
        Size::new(px(4), px(1))
    }

    fn pixel(&self, x: u32, y: u32) -> Option<Color> {
        if y != 0 {
            return None;
        }

        match x {
            0 => Some(Color::RED),
            1 => Some(Color::GREEN),
            2 => Some(Color::BLUE),
            3 => Some(Color::WHITE),
            _ => None,
        }
    }
}

#[test]
fn embedded_graphics_backend_draws_registered_image() {
    let bitmap = SolidTestImage {
        size: EgSize::new(4, 3),
        color: Rgb888::new(255, 0, 0),
    };

    let image = EmbeddedGraphicsImage::new(&bitmap);
    let mut images = ImageRegistry::<1>::default();
    let source = images.register(&image).unwrap();

    let mut display = MockDisplay::<Rgb888>::new();

    {
        let mut glyph_storage = [0; 4096];
        let mut fonts = test_font_resources(&mut glyph_storage);
        let mut painter = EmbeddedGraphicsPainter::new(&mut display, &mut fonts, images);

        painter
            .draw_image(
                source,
                Rect::new(Point::new(px(2), px(3)), source.size()),
                ImagePaint::default(),
                None,
            )
            .unwrap();
    }

    assert_eq!(
        display.get_pixel(EgPoint::new(2, 3)),
        Some(Rgb888::new(255, 0, 0))
    );
    assert_eq!(
        display.get_pixel(EgPoint::new(5, 5)),
        Some(Rgb888::new(255, 0, 0))
    );
    assert_eq!(display.get_pixel(EgPoint::new(1, 3)), None);
}

#[test]
fn embedded_graphics_backend_clips_image_to_layout_bounds() {
    let bitmap = SolidTestImage {
        size: EgSize::new(4, 4),
        color: Rgb888::new(0, 255, 0),
    };

    let image = EmbeddedGraphicsImage::new(&bitmap);
    let mut images = ImageRegistry::<1>::default();
    let source = images.register(&image).unwrap();

    let mut display = MockDisplay::<Rgb888>::new();

    {
        let mut glyph_storage = [0; 4096];
        let mut fonts = test_font_resources(&mut glyph_storage);
        let mut painter = EmbeddedGraphicsPainter::new(&mut display, &mut fonts, images);

        painter
            .draw_image(
                source,
                Rect::new(Point::new(px(10), px(10)), Size::new(px(2), px(2))),
                ImagePaint::default(),
                None,
            )
            .unwrap();
    }

    assert_eq!(
        display.get_pixel(EgPoint::new(10, 10)),
        Some(Rgb888::new(0, 255, 0))
    );
    assert_eq!(
        display.get_pixel(EgPoint::new(11, 11)),
        Some(Rgb888::new(0, 255, 0))
    );
    assert_eq!(display.get_pixel(EgPoint::new(12, 10)), None);
    assert_eq!(display.get_pixel(EgPoint::new(10, 12)), None);
}

#[test]
fn embedded_graphics_backend_combines_image_bounds_with_ancestor_clip() {
    let bitmap = SolidTestImage {
        size: EgSize::new(6, 4),
        color: Rgb888::new(0, 0, 255),
    };

    let image = EmbeddedGraphicsImage::new(&bitmap);
    let mut images = ImageRegistry::<1>::default();
    let source = images.register(&image).unwrap();

    let mut display = MockDisplay::<Rgb888>::new();

    {
        let mut glyph_storage = [0; 4096];
        let mut fonts = test_font_resources(&mut glyph_storage);
        let mut painter = EmbeddedGraphicsPainter::new(&mut display, &mut fonts, images);

        painter
            .draw_image(
                source,
                Rect::new(Point::new(px(4), px(4)), source.size()),
                ImagePaint::default(),
                Some(Rect::new(Point::new(px(5), px(5)), Size::new(px(2), px(2)))),
            )
            .unwrap();
    }

    assert_eq!(
        display.get_pixel(EgPoint::new(5, 5)),
        Some(Rgb888::new(0, 0, 255))
    );
    assert_eq!(
        display.get_pixel(EgPoint::new(6, 6)),
        Some(Rgb888::new(0, 0, 255))
    );
    assert_eq!(display.get_pixel(EgPoint::new(4, 4)), None);
    assert_eq!(display.get_pixel(EgPoint::new(7, 5)), None);
}

#[test]
fn embedded_graphics_backend_scales_image_with_contain() {
    let bitmap = SolidTestImage {
        size: EgSize::new(4, 2),
        color: Rgb888::new(255, 0, 0),
    };

    let image = EmbeddedGraphicsImage::new(&bitmap);
    let mut images = ImageRegistry::<1>::default();
    let source = images.register(&image).unwrap();

    let mut display = MockDisplay::<Rgb888>::new();

    {
        let mut glyph_storage = [0; 4096];
        let mut fonts = test_font_resources(&mut glyph_storage);
        let mut painter = EmbeddedGraphicsPainter::new(&mut display, &mut fonts, images);

        painter
            .draw_image(
                source,
                Rect::new(Point::ZERO, Size::new(px(8), px(8))),
                ImagePaint {
                    fit: ImageFit::Contain,
                    ..ImagePaint::default()
                },
                None,
            )
            .unwrap();
    }

    assert_eq!(
        display.get_pixel(EgPoint::new(0, 2)),
        Some(Rgb888::new(255, 0, 0))
    );
    assert_eq!(
        display.get_pixel(EgPoint::new(7, 5)),
        Some(Rgb888::new(255, 0, 0))
    );
    assert_eq!(display.get_pixel(EgPoint::new(0, 1)), None);
    assert_eq!(display.get_pixel(EgPoint::new(0, 6)), None);
}

fn draw_backend_test_canvas(bounds: Rect, painter: &mut dyn CanvasPainter) {
    painter.fill_rect(bounds, Color::RED);
    painter.fill_circle(Point::new(px(2), px(2)), px(1), Color::BLUE);
}

fn draw_oversized_canvas(_bounds: Rect, painter: &mut dyn CanvasPainter) {
    painter.fill_rect(
        Rect::new(Point::new(px(-10), px(-10)), Size::new(px(40), px(40))),
        Color::GREEN,
    );
}

#[test]
fn embedded_graphics_backend_draws_canvas_at_visual_origin() {
    let mut display = MockDisplay::<Rgb888>::new();
    display.set_allow_overdraw(true);

    {
        let mut glyph_storage = [0; 4096];
        let mut fonts = test_font_resources(&mut glyph_storage);
        let mut painter =
            EmbeddedGraphicsPainter::new(&mut display, &mut fonts, ImageRegistry::<0>::default());

        painter
            .draw_canvas(
                Rect::new(Point::new(px(10), px(20)), Size::new(px(8), px(6))),
                None,
                &mut draw_backend_test_canvas,
            )
            .unwrap();
    }

    assert_eq!(
        display.get_pixel(EgPoint::new(10, 20,)),
        Some(Rgb888::new(255, 0, 0,))
    );
    assert_eq!(
        display.get_pixel(EgPoint::new(12, 22,)),
        Some(Rgb888::new(0, 0, 255,))
    );
    assert_eq!(
        display.get_pixel(EgPoint::new(17, 25,)),
        Some(Rgb888::new(255, 0, 0,))
    );
    assert_eq!(display.get_pixel(EgPoint::new(9, 20,)), None);
}

#[test]
fn embedded_graphics_backend_clips_custom_drawing_to_canvas_bounds() {
    let mut display = MockDisplay::<Rgb888>::new();

    {
        let mut glyph_storage = [0; 4096];
        let mut fonts = test_font_resources(&mut glyph_storage);
        let mut painter =
            EmbeddedGraphicsPainter::new(&mut display, &mut fonts, ImageRegistry::<0>::default());

        painter
            .draw_canvas(
                Rect::new(Point::new(px(5), px(5)), Size::new(px(4), px(4))),
                None,
                &mut draw_oversized_canvas,
            )
            .unwrap();
    }

    assert_eq!(
        display.get_pixel(EgPoint::new(5, 5,)),
        Some(Rgb888::new(0, 255, 0,))
    );
    assert_eq!(
        display.get_pixel(EgPoint::new(8, 8,)),
        Some(Rgb888::new(0, 255, 0,))
    );
    assert_eq!(display.get_pixel(EgPoint::new(4, 5,)), None);
    assert_eq!(display.get_pixel(EgPoint::new(9, 5,)), None);
}

#[test]
fn embedded_graphics_backend_combines_canvas_and_ancestor_clipping() {
    let mut display = MockDisplay::<Rgb888>::new();

    {
        let mut glyph_storage = [0; 4096];
        let mut fonts = test_font_resources(&mut glyph_storage);
        let mut painter =
            EmbeddedGraphicsPainter::new(&mut display, &mut fonts, ImageRegistry::<0>::default());

        painter
            .draw_canvas(
                Rect::new(Point::new(px(5), px(5)), Size::new(px(10), px(10))),
                Some(Rect::new(Point::new(px(8), px(8)), Size::new(px(3), px(3)))),
                &mut draw_oversized_canvas,
            )
            .unwrap();
    }

    assert_eq!(
        display.get_pixel(EgPoint::new(8, 8,)),
        Some(Rgb888::new(0, 255, 0,))
    );
    assert_eq!(
        display.get_pixel(EgPoint::new(10, 10,)),
        Some(Rgb888::new(0, 255, 0,))
    );
    assert_eq!(display.get_pixel(EgPoint::new(7, 8,)), None);
    assert_eq!(display.get_pixel(EgPoint::new(11, 8,)), None);
}

#[test]
fn embedded_graphics_backend_clears_only_partial_damage() {
    let mut display = MockDisplay::<Rgb888>::new();

    display.set_allow_overdraw(true);

    display
        .draw_iter([
            Pixel(EgPoint::new(2, 2), Rgb888::new(255, 0, 0)),
            Pixel(EgPoint::new(8, 8), Rgb888::new(255, 0, 0)),
        ])
        .unwrap();

    {
        let mut glyph_storage = [0; 4096];
        let mut fonts = test_font_resources(&mut glyph_storage);
        let mut painter =
            EmbeddedGraphicsPainter::new(&mut display, &mut fonts, ImageRegistry::<0>::default());

        painter
            .clear_damage(
                DamageRegion::from_rect(Rect::new(
                    Point::new(px(0), px(0)),
                    Size::new(px(5), px(5)),
                )),
                Color::BLACK,
            )
            .unwrap();
    }

    // inside damage was restored to the clear color.
    assert_eq!(
        display.get_pixel(EgPoint::new(2, 2),),
        Some(Rgb888::new(0, 0, 0),),
    );
    // pixels outside damage were untouched.
    assert_eq!(
        display.get_pixel(EgPoint::new(8, 8),),
        Some(Rgb888::new(255, 0, 0),),
    );
}

#[test]
fn embedded_graphics_backend_full_damage_clears_target() {
    let mut display = MockDisplay::<Rgb888>::new();

    display.set_allow_overdraw(true);

    display
        .draw_iter([
            Pixel(EgPoint::new(2, 2), Rgb888::new(255, 0, 0)),
            Pixel(EgPoint::new(8, 8), Rgb888::new(0, 255, 0)),
        ])
        .unwrap();

    {
        let mut glyph_storage = [0; 4096];
        let mut fonts = test_font_resources(&mut glyph_storage);
        let mut painter =
            EmbeddedGraphicsPainter::new(&mut display, &mut fonts, ImageRegistry::<0>::default());

        painter
            .clear_damage(DamageRegion::full(), Color::BLUE)
            .unwrap();
    }

    assert_eq!(
        display.get_pixel(EgPoint::new(2, 2),),
        Some(Rgb888::new(0, 0, 255),),
    );
    assert_eq!(
        display.get_pixel(EgPoint::new(8, 8),),
        Some(Rgb888::new(0, 0, 255),),
    );
}

#[test]
fn embedded_graphics_backend_draws_text_through_font_resources() {
    let mut display = MockDisplay::<Rgb888>::new();
    let mut glyph_storage = [0u8; 4096];
    let mut fonts = test_font_resources(&mut glyph_storage);

    {
        let mut painter =
            EmbeddedGraphicsPainter::new(&mut display, &mut fonts, ImageRegistry::<0>::default());

        painter
            .draw_text(
                "A",
                Rect::new(Point::ZERO, Size::new(px(20), px(20))),
                ResolvedTextStyle::default(),
                None,
            )
            .unwrap();
    }

    assert!(fonts.glyph_cache_used_bytes() > 0,);

    let has_black_pixel = (0..10).any(|y| {
        (0..6).any(|x| display.get_pixel(EgPoint::new(x, y)) == Some(Rgb888::new(0, 0, 0)))
    });

    assert!(has_black_pixel,);
}

#[test]
fn alpha_blending_preserves_coverage_endpoints() {
    let black = Rgb888::new(0, 0, 0);
    let white = Rgb888::new(255, 255, 255);

    assert_eq!(alpha_blend_rgb888(black, white, 0,), white,);
    assert_eq!(alpha_blend_rgb888(black, white, 255,), black,);
}

#[test]
fn alpha_blending_produces_intermediate_gray() {
    let black = Rgb888::new(0, 0, 0);
    let white = Rgb888::new(255, 255, 255);

    assert_eq!(
        alpha_blend_rgb888(black, white, 128,),
        Rgb888::new(127, 127, 127,),
    );
}

#[test]
fn ordered_dither_half_coverage_draws_half_of_matrix() {
    let mut drawn = 0usize;
    for y in 0..4 {
        for x in 0..4 {
            if ordered_dither_accepts(128, EgPoint::new(x, y)) {
                drawn += 1;
            }
        }
    }

    assert_eq!(drawn, 8,);
}

#[test]
fn ordered_dither_is_stable_in_absolute_coordinates() {
    for y in 0..4 {
        for x in 0..4 {
            let first = ordered_dither_accepts(93, EgPoint::new(x, y));
            let repeated = ordered_dither_accepts(93, EgPoint::new(x + 4, y + 4));

            assert_eq!(first, repeated,);
        }
    }
}

#[test]
fn rtl_start_and_end_alignment_are_mirrored() {
    let bounds = Rect::new(Point::new(px(10), px(5)), Size::new(px(100), px(20)));

    assert_eq!(
        aligned_line_x(bounds, px(40), TextAlign::Start, TextDirection::RightToLeft,),
        px(70),
    );
    assert_eq!(
        aligned_line_x(
            bounds,
            px(40),
            TextAlign::Center,
            TextDirection::RightToLeft,
        ),
        px(40),
    );
    assert_eq!(
        aligned_line_x(bounds, px(40), TextAlign::End, TextDirection::RightToLeft,),
        px(10),
    );
}

#[test]
fn embedded_graphics_backend_cover_position_selects_horizontal_crop() {
    let image = HorizontalStripImage;

    let mut images = ImageRegistry::<1>::default();
    let source = images.register(&image).unwrap();

    let bounds = Rect::new(Point::ZERO, Size::new(px(2), px(2)));

    let mut left_display = MockDisplay::<Rgb888>::new();

    {
        let mut glyph_storage = [0; 4096];
        let mut fonts = test_font_resources(&mut glyph_storage);
        let mut painter = EmbeddedGraphicsPainter::new(&mut left_display, &mut fonts, images);

        painter
            .draw_image(
                source,
                bounds,
                ImagePaint {
                    fit: ImageFit::Cover,
                    position: ImagePosition::Left,
                    sampling: ImageSampling::Nearest,
                },
                None,
            )
            .unwrap();
    }

    let mut right_display = MockDisplay::<Rgb888>::new();

    {
        let mut glyph_storage = [0; 4096];
        let mut fonts = test_font_resources(&mut glyph_storage);
        let mut painter = EmbeddedGraphicsPainter::new(&mut right_display, &mut fonts, images);

        painter
            .draw_image(
                source,
                bounds,
                ImagePaint {
                    fit: ImageFit::Cover,
                    position: ImagePosition::Right,
                    sampling: ImageSampling::Nearest,
                },
                None,
            )
            .unwrap();
    }

    assert_eq!(
        left_display.get_pixel(EgPoint::new(0, 0)),
        Some(Rgb888::new(255, 0, 0))
    );

    assert_eq!(
        right_display.get_pixel(EgPoint::new(0, 0)),
        Some(Rgb888::new(255, 255, 255))
    );
}

#[test]
fn embedded_graphics_backend_bilinear_sampling_blends_neighboring_pixels() {
    let image = FourColorImage;

    let mut images = ImageRegistry::<1>::default();
    let source = images.register(&image).unwrap();

    let mut display = MockDisplay::<Rgb888>::new();

    {
        let mut glyph_storage = [0; 4096];
        let mut fonts = test_font_resources(&mut glyph_storage);
        let mut painter = EmbeddedGraphicsPainter::new(&mut display, &mut fonts, images);

        painter
            .draw_image(
                source,
                Rect::new(Point::ZERO, Size::new(px(3), px(3))),
                ImagePaint {
                    fit: ImageFit::Fill,
                    sampling: ImageSampling::Bilinear,
                    ..ImagePaint::default()
                },
                None,
            )
            .unwrap();
    }

    assert_eq!(
        display.get_pixel(EgPoint::new(0, 0)),
        Some(Rgb888::new(0, 0, 0))
    );

    assert_eq!(
        display.get_pixel(EgPoint::new(1, 1)),
        Some(Rgb888::new(64, 64, 64))
    );

    assert_eq!(
        display.get_pixel(EgPoint::new(2, 2)),
        Some(Rgb888::new(0, 0, 255))
    );
}

#[test]
fn embedded_graphics_backend_bilinear_sampling_is_stable_when_clipped() {
    let image = FourColorImage;

    let mut images = ImageRegistry::<1>::default();
    let source = images.register(&image).unwrap();

    let bounds = Rect::new(Point::ZERO, Size::new(px(5), px(5)));

    let paint = ImagePaint {
        fit: ImageFit::Fill,
        sampling: ImageSampling::Bilinear,
        ..ImagePaint::default()
    };

    let mut full_display = MockDisplay::<Rgb888>::new();

    {
        let mut glyph_storage = [0; 4096];
        let mut fonts = test_font_resources(&mut glyph_storage);
        let mut painter = EmbeddedGraphicsPainter::new(&mut full_display, &mut fonts, images);

        painter.draw_image(source, bounds, paint, None).unwrap();
    }

    let clip = Rect::new(Point::new(px(1), px(1)), Size::new(px(3), px(3)));

    let mut clipped_display = MockDisplay::<Rgb888>::new();

    {
        let mut glyph_storage = [0; 4096];
        let mut fonts = test_font_resources(&mut glyph_storage);
        let mut painter = EmbeddedGraphicsPainter::new(&mut clipped_display, &mut fonts, images);

        painter
            .draw_image(source, bounds, paint, Some(clip))
            .unwrap();
    }

    for y in 1..4 {
        for x in 1..4 {
            assert_eq!(
                clipped_display.get_pixel(EgPoint::new(x, y)),
                full_display.get_pixel(EgPoint::new(x, y)),
                "clipped bilinear sample differs at ({x}, {y})",
            );
        }
    }
}
