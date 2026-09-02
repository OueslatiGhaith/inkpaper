use embedded_graphics::{pixelcolor::Rgb888 as EgRgb888, prelude::DrawTarget as EgDrawTarget};

use crate::{
    FontFace, FontId, FontRegistry, LineHeight, Pixels, Point, Rect, ResolvedTextStyle, ShapeState,
    ShapedGlyph, ShapedRun, SimpleShaper, TextAlign, TextDirection, px,
    resources::RuntimeResources,
    text_layout::{ELLIPSIS, for_each_visible_text_line_with_boundaries},
};

use super::{CoverageMode, EmbeddedGraphicsError, coverage::draw_coverage_bitmap, to_rgb888};

const SHAPED_LINE_GLYPH_CAPACITY: usize = 128;

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_text_to<
    D,
    const FONTS: usize,
    const GLYPH_SLOTS: usize,
    const GLYPH_BYTES: usize,
    const IMAGES: usize,
>(
    target: &mut D,
    text: &str,
    bounds: Rect,
    clip: Rect,
    registry: &FontRegistry<'_, FONTS>,
    resources: &mut RuntimeResources<'_, FONTS, GLYPH_SLOTS, GLYPH_BYTES, IMAGES>,
    font_id: FontId,
    font: &dyn FontFace,
    style: ResolvedTextStyle,
    coverage_mode: CoverageMode<D>,
) -> Result<(), EmbeddedGraphicsError<D::Error>>
where
    D: EgDrawTarget,
    D::Color: From<EgRgb888>,
{
    if text.is_empty() {
        return Ok(());
    }

    let size_px = font_size_px(style);
    let line_advance = text_line_advance(font, size_px, style);

    let baseline_offset = font.metrics(size_px).ascent;

    let color = to_rgb888(style.color);
    let shaper = SimpleShaper::new();

    let mut y = bounds.origin.y;
    let mut error = None;

    for_each_visible_text_line_with_boundaries(
        text,
        style.wrap,
        bounds.width(),
        style.max_lines,
        style.overflow,
        |line, from| shaper.next_cluster_boundary(registry, font_id, size_px, line, from),
        |line| measure_shaped_line(registry, font_id, size_px, line),
        |line| measure_shaped_line_with_ellipsis(registry, font_id, size_px, line),
        |line| {
            if error.is_some() {
                return;
            }

            let mut glyphs = [ShapedGlyph::EMPTY; SHAPED_LINE_GLYPH_CAPACITY];
            let mut shape_state = ShapeState::new();

            let text_summary = match shaper.shape_piece_into(
                registry,
                font_id,
                size_px,
                line.text,
                &mut shape_state,
                &mut glyphs,
            ) {
                Ok(summary) => summary,
                Err(shape_error) => {
                    error = Some(EmbeddedGraphicsError::Shape(shape_error));
                    return;
                }
            };

            let text_glyph_count = text_summary.glyph_count();
            let mut glyph_count = text_glyph_count;

            if line.ellipsis {
                let ellipsis_summary = match shaper.shape_piece_into(
                    registry,
                    font_id,
                    size_px,
                    ELLIPSIS,
                    &mut shape_state,
                    &mut glyphs[glyph_count..],
                ) {
                    Ok(summary) => summary,
                    Err(shape_error) => {
                        error = Some(EmbeddedGraphicsError::Shape(shape_error));
                        return;
                    }
                };

                glyph_count = glyph_count.saturating_add(ellipsis_summary.glyph_count());
            }

            let run = match shaper.visual_order(
                registry,
                size_px,
                line.text,
                text_glyph_count,
                &mut glyphs[..glyph_count],
            ) {
                Ok(run) => run,
                Err(shape_error) => {
                    error = Some(EmbeddedGraphicsError::Shape(shape_error));
                    return;
                }
            };

            debug_assert_eq!(run.advance(), line.width);

            let mut pen_x = aligned_line_x(bounds, line.width, style.align, run.direction());
            let baseline = y + baseline_offset;

            if let Err(draw_error) = draw_shaped_run(
                target,
                resources,
                size_px,
                &run,
                baseline,
                color,
                clip,
                coverage_mode,
                &mut pen_x,
            ) {
                error = Some(draw_error);
                return;
            }

            y += line_advance;
        },
    );

    match error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_shaped_run<
    D,
    const FONTS: usize,
    const GLYPH_SLOTS: usize,
    const GLYPH_BYTES: usize,
    const IMAGES: usize,
>(
    target: &mut D,
    resources: &mut RuntimeResources<'_, FONTS, GLYPH_SLOTS, GLYPH_BYTES, IMAGES>,
    size_px: u16,
    run: &ShapedRun<'_>,
    baseline: Pixels,
    color: EgRgb888,
    clip: Rect,
    coverage_mode: CoverageMode<D>,
    pen_x: &mut Pixels,
) -> Result<(), EmbeddedGraphicsError<D::Error>>
where
    D: EgDrawTarget,
    D::Color: From<EgRgb888>,
{
    for shaped in run.glyphs().iter().copied() {
        let bitmap = resources
            .glyph_bitmap(shaped.font(), shaped.glyph(), size_px)
            .map_err(EmbeddedGraphicsError::Font)?;

        let metrics = bitmap.metrics();
        let offset = shaped.offset();

        let origin = Point::new(
            *pen_x + offset.x + metrics.bearing_x,
            baseline + offset.y + metrics.bearing_y,
        );

        draw_coverage_bitmap(target, &bitmap, origin, color, clip, coverage_mode)?;

        *pen_x += shaped.advance();
    }

    Ok(())
}

fn measure_shaped_line<const FONTS: usize>(
    registry: &FontRegistry<'_, FONTS>,
    font: FontId,
    size_px: u16,
    text: &str,
) -> Pixels {
    let mut glyphs = [ShapedGlyph::EMPTY; SHAPED_LINE_GLYPH_CAPACITY];

    match SimpleShaper::new().measure(registry, font, size_px, text, &mut glyphs) {
        Ok(summary) => summary.advance(),
        // textMeasurer cannot currently surface ShapeError.
        // treat an unmeasurable candidate as wider than any available line. Word wrapping
        // can then keep reducing the candidate until it fits the bounded shaper.
        Err(_) => Pixels::MAX,
    }
}

fn measure_shaped_line_with_ellipsis<const FONTS: usize>(
    registry: &FontRegistry<'_, FONTS>,
    font: FontId,
    size_px: u16,
    text: &str,
) -> Pixels {
    let shaper = SimpleShaper::new();
    let mut glyphs = [ShapedGlyph::EMPTY; SHAPED_LINE_GLYPH_CAPACITY];
    let mut state = ShapeState::new();

    let text_summary =
        match shaper.shape_piece_into(registry, font, size_px, text, &mut state, &mut glyphs) {
            Ok(summary) => summary,
            Err(_) => return Pixels::MAX,
        };

    let text_glyph_count = text_summary.glyph_count();
    let mut glyph_count = text_glyph_count;
    let ellipsis_summary = match shaper.shape_piece_into(
        registry,
        font,
        size_px,
        ELLIPSIS,
        &mut state,
        &mut glyphs[glyph_count..],
    ) {
        Ok(summary) => summary,
        Err(_) => return Pixels::MAX,
    };

    glyph_count = glyph_count.saturating_add(ellipsis_summary.glyph_count());

    match shaper.visual_order(
        registry,
        size_px,
        text,
        text_glyph_count,
        &mut glyphs[..glyph_count],
    ) {
        Ok(run) => run.advance(),
        Err(_) => Pixels::MAX,
    }
}

fn text_line_advance(font: &dyn FontFace, size_px: u16, style: ResolvedTextStyle) -> Pixels {
    match style.line_height {
        LineHeight::Normal => font.metrics(size_px).line_height().non_negative(),
        LineHeight::Pixels(height) => height.non_negative(),
    }
}

pub(super) fn aligned_line_x(
    bounds: Rect,
    line_width: Pixels,
    align: TextAlign,
    direction: TextDirection,
) -> Pixels {
    use TextAlign::*;
    use TextDirection::*;

    let remaining = (bounds.width() - line_width).non_negative();

    match (align, direction) {
        (Center, _) => bounds.origin.x + remaining / 2,
        (Start, LeftToRight) | (End, RightToLeft) => bounds.origin.x,
        (End, LeftToRight) | (Start, RightToLeft) => bounds.origin.x + remaining,
    }
}

fn font_size_px(style: ResolvedTextStyle) -> u16 {
    let size = style.font_size.max(px(1)).get();
    u16::try_from(size).unwrap_or(u16::MAX)
}
