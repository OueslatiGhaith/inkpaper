extern crate alloc;

use embedded_graphics::{pixelcolor::GrayColor, prelude::DrawTarget as _};
use inkpaper_ui::{
    CanvasPainter, Color, Painter, Point, Rect, Size,
    backend::{EInkOrderedCoverageBitmap, EInkPainter, EInkUiMode, Gray2},
    px,
};

#[path = "../src/firmware/framebuffer.rs"]
mod framebuffer;

use framebuffer::{Framebuffer, FramebufferStorage, LOGICAL_HEIGHT, LOGICAL_WIDTH, Orientation};

const WIDTH: u16 = 6;
const HEIGHT: u16 = 5;

const ZERO_COVERAGE: [u8; 30] = [0; 30];

const FULL_COVERAGE: [u8; 30] = [255; 30];

const MIXED_COVERAGE: [u8; 30] = [
    0, 8, 9, 24, 25, 255, 40, 41, 56, 57, 72, 73, 88, 89, 104, 105, 120, 121, 136, 137, 152, 153,
    168, 169, 184, 185, 200, 201, 232, 249,
];

fn point(x: i32, y: i32) -> Point {
    Point::new(px(x), px(y))
}

fn rect(x: i32, y: i32, width: i32, height: i32) -> Rect {
    Rect::new(point(x, y), Size::new(px(width), px(height)))
}

fn full_screen_clip() -> Rect {
    rect(0, 0, LOGICAL_WIDTH as i32, LOGICAL_HEIGHT as i32)
}

fn render_generic(
    coverage: &[u8],
    width: u16,
    height: u16,
    origin: Point,
    clip: Rect,
    foreground: Color,
    background: Gray2,
) -> FramebufferStorage {
    let mut storage = FramebufferStorage::white();

    {
        let mut framebuffer = Framebuffer::new(&mut storage, Orientation::Portrait);

        framebuffer.clear(background).unwrap();

        let bounds = Rect::new(
            origin,
            Size::new(px(i32::from(width)), px(i32::from(height))),
        );

        let mut painter = EInkPainter::new(&mut framebuffer).with_ui_mode(EInkUiMode::BinaryDither);

        let mut draw = |_: Rect, canvas: &mut dyn CanvasPainter| {
            for y in 0..height {
                for x in 0..width {
                    let index = usize::from(y) * usize::from(width) + usize::from(x);

                    canvas.fill_pixel_coverage(
                        point(i32::from(x), i32::from(y)),
                        foreground,
                        coverage[index],
                    );
                }
            }
        };

        painter.draw_canvas(bounds, Some(clip), &mut draw).unwrap();
    }

    storage
}

fn render_fast(
    coverage: &[u8],
    width: u16,
    height: u16,
    origin: Point,
    clip: Rect,
    foreground: Gray2,
    background: Gray2,
) -> (FramebufferStorage, u64) {
    let mut storage = FramebufferStorage::white();

    let accepted = {
        let mut framebuffer = Framebuffer::new(&mut storage, Orientation::Portrait);

        framebuffer.clear(background).unwrap();

        framebuffer
            .draw_ordered_coverage_bitmap(EInkOrderedCoverageBitmap::new(
                coverage, width, height, origin, foreground, clip,
            ))
            .expect("portrait binary coverage should use the fast path")
    };

    (storage, accepted)
}

fn assert_fast_matches_generic(
    name: &str,
    coverage: &[u8],
    origin: Point,
    clip: Rect,
    foreground: Color,
    foreground_gray: Gray2,
    background: Gray2,
) {
    let generic = render_generic(
        coverage, WIDTH, HEIGHT, origin, clip, foreground, background,
    );

    let (fast, _) = render_fast(
        coverage,
        WIDTH,
        HEIGHT,
        origin,
        clip,
        foreground_gray,
        background,
    );

    assert_eq!(generic.lsb(), fast.lsb(), "{name}: LSB plane differs",);

    assert_eq!(generic.msb(), fast.msb(), "{name}: MSB plane differs",);
}

#[test]
fn fast_ordered_coverage_matches_generic_inside_clip() {
    assert_fast_matches_generic(
        "inside",
        &MIXED_COVERAGE,
        point(21, 37),
        full_screen_clip(),
        Color::BLACK,
        Gray2::new(0),
        Gray2::new(3),
    );
}

#[test]
fn fast_ordered_coverage_matches_generic_for_each_clip_edge() {
    let origin = point(21, 37);

    let cases = [
        ("left", rect(23, 37, 4, 5)),
        ("right", rect(21, 37, 4, 5)),
        ("top", rect(21, 39, 6, 3)),
        ("bottom", rect(21, 37, 6, 3)),
    ];

    for (name, clip) in cases {
        assert_fast_matches_generic(
            name,
            &MIXED_COVERAGE,
            origin,
            clip,
            Color::BLACK,
            Gray2::new(0),
            Gray2::new(3),
        );
    }
}

#[test]
fn fast_ordered_coverage_matches_generic_at_all_bayer_phases() {
    let clip = full_screen_clip();

    for y_phase in 0..4 {
        for x_phase in 0..4 {
            let origin = point(20 + x_phase, 36 + y_phase);

            assert_fast_matches_generic(
                "bayer phase",
                &MIXED_COVERAGE,
                origin,
                clip,
                Color::BLACK,
                Gray2::new(0),
                Gray2::new(3),
            );
        }
    }
}

#[test]
fn fast_ordered_coverage_matches_generic_at_display_edges() {
    let clip = full_screen_clip();

    let cases = [
        ("left", point(-2, 37)),
        ("right", point(478, 37)),
        ("top", point(21, -2)),
        ("bottom", point(21, 798)),
    ];

    for (name, origin) in cases {
        assert_fast_matches_generic(
            name,
            &MIXED_COVERAGE,
            origin,
            clip,
            Color::BLACK,
            Gray2::new(0),
            Gray2::new(3),
        );
    }
}

#[test]
fn fast_ordered_coverage_matches_zero_coverage() {
    let origin = point(23, 41);
    let clip = full_screen_clip();

    assert_fast_matches_generic(
        "zero",
        &ZERO_COVERAGE,
        origin,
        clip,
        Color::BLACK,
        Gray2::new(0),
        Gray2::new(3),
    );

    let (_, accepted) = render_fast(
        &ZERO_COVERAGE,
        WIDTH,
        HEIGHT,
        origin,
        clip,
        Gray2::new(0),
        Gray2::new(3),
    );

    assert_eq!(accepted, 0);
}

#[test]
fn fast_ordered_coverage_matches_full_coverage() {
    let origin = point(23, 41);
    let clip = full_screen_clip();

    assert_fast_matches_generic(
        "full",
        &FULL_COVERAGE,
        origin,
        clip,
        Color::BLACK,
        Gray2::new(0),
        Gray2::new(3),
    );

    let (_, accepted) = render_fast(
        &FULL_COVERAGE,
        WIDTH,
        HEIGHT,
        origin,
        clip,
        Gray2::new(0),
        Gray2::new(3),
    );

    assert_eq!(accepted, u64::from(WIDTH) * u64::from(HEIGHT),);
}

#[test]
fn fast_ordered_coverage_matches_partial_coverage() {
    assert_fast_matches_generic(
        "partial",
        &MIXED_COVERAGE,
        point(23, 41),
        full_screen_clip(),
        Color::BLACK,
        Gray2::new(0),
        Gray2::new(3),
    );
}

#[test]
fn fast_ordered_coverage_matches_white_foreground_on_black() {
    assert_fast_matches_generic(
        "white",
        &MIXED_COVERAGE,
        point(17, 29),
        full_screen_clip(),
        Color::WHITE,
        Gray2::new(3),
        Gray2::new(0),
    );
}

#[test]
fn fast_ordered_coverage_declines_non_portrait_orientation() {
    let mut storage = FramebufferStorage::white();

    let mut framebuffer = Framebuffer::new(&mut storage, Orientation::PortraitInverted);

    let request = EInkOrderedCoverageBitmap::new(
        &MIXED_COVERAGE,
        WIDTH,
        HEIGHT,
        point(20, 30),
        Gray2::new(0),
        full_screen_clip(),
    );

    assert_eq!(framebuffer.draw_ordered_coverage_bitmap(request), None,);
}

#[test]
fn fast_ordered_coverage_declines_native_gray_foreground() {
    let mut storage = FramebufferStorage::white();

    let mut framebuffer = Framebuffer::new(&mut storage, Orientation::Portrait);

    let request = EInkOrderedCoverageBitmap::new(
        &MIXED_COVERAGE,
        WIDTH,
        HEIGHT,
        point(20, 30),
        Gray2::new(1),
        full_screen_clip(),
    );

    assert_eq!(framebuffer.draw_ordered_coverage_bitmap(request), None,);
}
