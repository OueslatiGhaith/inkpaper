//! Lays out, shapes and rasterizes text into positioned glyph bitmaps, leaving
//! only putting their coverage down to a [`Painter`].

use crate::{
    Color, FontInstance, FontRegistry, GlyphCache, LineHeight, PaintError, Painter, Pixels, Point,
    Rect, ResolvedFont, ResolvedTextStyle, ShapeState, ShapedGlyph, ShapedRun, SimpleShaper,
    TextAlign, TextDirection,
    pair_cache::PairPositioningCache,
    px,
    text_layout::{ELLIPSIS, for_each_visible_text_line_with_boundaries},
};

const SHAPED_LINE_GLYPH_CAPACITY: usize = 128;

/// paints `text` wrapped, truncated and aligned inside `bounds`, returning how many
/// glyphs it shaped
#[allow(clippy::too_many_arguments)]
pub(crate) fn paint_text<
    P,
    const FONTS: usize,
    const GLYPH_SLOTS: usize,
    const GLYPH_BYTES: usize,
>(
    painter: &mut P,
    text: &str,
    bounds: Rect,
    clip: Rect,
    registry: &FontRegistry<'_, FONTS>,
    glyph_cache: &mut GlyphCache<GLYPH_SLOTS, GLYPH_BYTES>,
    font: ResolvedFont<'_>,
    style: ResolvedTextStyle,
) -> Result<u64, PaintError<P::Error>>
where
    P: Painter,
{
    if text.is_empty() {
        return Ok(0);
    }

    let font_instance = font.instance();

    let size_px = font_size_px(style);
    let line_advance = text_line_advance(font, size_px, style);

    let baseline_offset = font.metrics(size_px).ascent;
    let color = style.color;

    let shaper = SimpleShaper::with_properties(font.properties());

    let mut y = bounds.origin.y;
    let mut error = None;
    let mut shaped_glyphs = 0u64;

    for_each_visible_text_line_with_boundaries(
        text,
        style.wrap,
        bounds.width(),
        style.max_lines,
        style.overflow,
        |line, from| {
            shaper.next_cluster_boundary(registry, font_instance.font(), size_px, line, from)
        },
        |line| measure_shaped_line(registry, font_instance, size_px, line),
        |line| measure_shaped_line_with_ellipsis(registry, font_instance, size_px, line),
        |line| {
            if error.is_some() {
                return;
            }

            let mut glyphs = [ShapedGlyph::EMPTY; SHAPED_LINE_GLYPH_CAPACITY];

            let mut shape_state = ShapeState::new();

            let text_summary = match shaper.shape_piece_into(
                registry,
                font_instance.font(),
                size_px,
                line.text,
                &mut shape_state,
                &mut glyphs,
            ) {
                Ok(summary) => summary,
                Err(shape_error) => {
                    error = Some(PaintError::Shape(shape_error));
                    return;
                }
            };

            let text_glyph_count = text_summary.glyph_count();

            let mut glyph_count = text_glyph_count;

            if line.ellipsis {
                let ellipsis_summary = match shaper.shape_piece_into(
                    registry,
                    font_instance.font(),
                    size_px,
                    ELLIPSIS,
                    &mut shape_state,
                    &mut glyphs[glyph_count..],
                ) {
                    Ok(summary) => summary,
                    Err(shape_error) => {
                        error = Some(PaintError::Shape(shape_error));
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
                    error = Some(PaintError::Shape(shape_error));
                    return;
                }
            };

            debug_assert_eq!(run.advance(), line.width);

            let mut pen_x = aligned_line_x(bounds, line.width, style.align, run.direction());

            let baseline = y + baseline_offset;

            if let Err(draw_error) = paint_shaped_run(
                painter,
                registry,
                glyph_cache,
                size_px,
                &run,
                baseline,
                color,
                clip,
                &mut pen_x,
                WordSpacing::NONE,
            ) {
                error = Some(draw_error);
                return;
            }

            shaped_glyphs = shaped_glyphs.saturating_add(glyph_count as u64);

            y += line_advance;
        },
    );

    match error {
        Some(error) => Err(error),
        None => Ok(shaped_glyphs),
    }
}

/// paints a single run whose position the caller already decided, returning how
/// many glyphs it shaped
#[allow(clippy::too_many_arguments)]
pub(crate) fn paint_text_run<
    P,
    const FONTS: usize,
    const GLYPH_SLOTS: usize,
    const GLYPH_BYTES: usize,
    const PAIR_SLOTS: usize,
>(
    painter: &mut P,
    text: &str,
    bounds: Rect,
    clip: Rect,
    registry: &FontRegistry<'_, FONTS>,
    glyph_cache: &mut GlyphCache<GLYPH_SLOTS, GLYPH_BYTES>,
    font: ResolvedFont<'_>,
    style: ResolvedTextStyle,
    pair_positioning_cache: &mut PairPositioningCache<PAIR_SLOTS>,
) -> Result<u64, PaintError<P::Error>>
where
    P: Painter,
{
    if text.is_empty() {
        return Ok(0);
    }

    let font_instance = font.instance();
    let size_px = font_size_px(style);

    let baseline_offset = font.metrics(size_px).ascent;
    let baseline = bounds.origin.y + baseline_offset;

    let shaper = SimpleShaper::with_properties(font.properties());

    let mut glyphs = [ShapedGlyph::EMPTY; SHAPED_LINE_GLYPH_CAPACITY];

    let run = shaper
        .shape_into_with_pair_positioning_cache(
            registry,
            font_instance.font(),
            size_px,
            text,
            &mut glyphs,
            Some(pair_positioning_cache),
        )
        .map_err(PaintError::Shape)?;

    let glyph_count = run.len();

    // this run was already positioned by its caller. Do not perform another
    // alignment/layout pass here.
    let mut pen_x = bounds.origin.x;

    paint_shaped_run(
        painter,
        registry,
        glyph_cache,
        size_px,
        &run,
        baseline,
        style.color,
        clip,
        &mut pen_x,
        WordSpacing::new(text, style.word_spacing),
    )?;

    if style.underline
        && let Some(underline) = underline_rect(bounds.origin.x, pen_x, baseline, size_px)
    {
        painter
            .fill_rect(underline, style.color, clip)
            .map_err(PaintError::Painter)?;
    }

    Ok(u64::try_from(glyph_count).unwrap_or(u64::MAX))
}

#[allow(clippy::too_many_arguments)]
fn paint_shaped_run<P, const FONTS: usize, const GLYPH_SLOTS: usize, const GLYPH_BYTES: usize>(
    painter: &mut P,
    registry: &FontRegistry<'_, FONTS>,
    glyph_cache: &mut GlyphCache<GLYPH_SLOTS, GLYPH_BYTES>,
    size_px: u16,
    run: &ShapedRun<'_>,
    baseline: Pixels,
    color: Color,
    clip: Rect,
    pen_x: &mut Pixels,
    word_spacing: WordSpacing<'_>,
) -> Result<(), PaintError<P::Error>>
where
    P: Painter,
{
    for shaped in run.glyphs().iter().copied() {
        let bitmap = glyph_cache
            .get_or_rasterize(registry, shaped.font_instance(), shaped.glyph(), size_px)
            .map_err(PaintError::Font)?;

        let metrics = bitmap.metrics();
        let offset = shaped.offset();

        let origin = Point::new(
            *pen_x + offset.x + metrics.bearing_x,
            baseline + offset.y + metrics.bearing_y,
        );

        painter
            .draw_glyph(&bitmap, origin, color, clip)
            .map_err(PaintError::Painter)?;

        *pen_x += shaped.advance() + word_spacing.after(shaped);
    }

    Ok(())
}

fn measure_shaped_line<const FONTS: usize>(
    registry: &FontRegistry<'_, FONTS>,
    font: FontInstance,
    size_px: u16,
    text: &str,
) -> Pixels {
    let mut glyphs = [ShapedGlyph::EMPTY; SHAPED_LINE_GLYPH_CAPACITY];

    let shaper = SimpleShaper::with_properties(font.properties());

    match shaper.measure(registry, font.font(), size_px, text, &mut glyphs) {
        Ok(summary) => summary.advance(),
        // textMeasurer cannot currently surface ShapeError.
        // treat an unmeasurable candidate as wider than any available line. Word wrapping
        // can then keep reducing the candidate until it fits the bounded shaper.
        Err(_) => Pixels::MAX,
    }
}

fn measure_shaped_line_with_ellipsis<const FONTS: usize>(
    registry: &FontRegistry<'_, FONTS>,
    font: FontInstance,
    size_px: u16,
    text: &str,
) -> Pixels {
    let shaper = SimpleShaper::with_properties(font.properties());
    let font = font.font();
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

fn text_line_advance(font: ResolvedFont<'_>, size_px: u16, style: ResolvedTextStyle) -> Pixels {
    match style.line_height {
        LineHeight::Normal => font.metrics(size_px).line_height().non_negative(),
        LineHeight::Pixels(height) => height.non_negative(),
    }
}

pub(crate) fn aligned_line_x(
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

/// extra advance after each U+0020 space of a run's source text
#[derive(Debug, Clone, Copy)]
struct WordSpacing<'a> {
    text: &'a str,
    extra: Pixels,
}

impl<'a> WordSpacing<'a> {
    const NONE: Self = Self::new("", Pixels::ZERO);

    const fn new(text: &'a str, extra: Pixels) -> Self {
        Self { text, extra }
    }

    fn after(self, glyph: ShapedGlyph) -> Pixels {
        if self.text.as_bytes().get(glyph.cluster()) == Some(&b' ') {
            self.extra
        } else {
            Pixels::ZERO
        }
    }
}

/// A text run's underline, from `start` to `end` just below the baseline like
/// crosspoint's, thickening with the font size.
fn underline_rect(start: Pixels, end: Pixels, baseline: Pixels, size_px: u16) -> Option<Rect> {
    let width = end - start;

    if width.is_non_positive() {
        return None;
    }

    let thickness = px(i32::from((size_px / 20).max(1)));

    Some(Rect::new(
        Point::new(start, baseline + px(2)),
        crate::Size::new(width, thickness),
    ))
}
