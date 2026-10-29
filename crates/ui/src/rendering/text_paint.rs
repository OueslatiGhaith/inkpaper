//! Lays out, shapes and rasterizes text into positioned glyph bitmaps, leaving
//! only putting their coverage down to a [`Painter`].

use crate::{
    FontRegistry, GlyphCache, LineHeight, PaintError, Painter, Pixels, Point, Rect, ResolvedFont,
    ResolvedTextStyle, ShapedGlyph, SimpleShaper, TextAlign, TextDirection,
    font::line_width,
    pair_cache::PairPositioningCache,
    paragraph_direction, px,
    text_layout::{ELLIPSIS, for_each_visible_text_line_with_boundaries},
};

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

    let size_px = font_size_px(style);
    let line_advance = text_line_advance(font, size_px, style);

    let baseline_offset = font.metrics(size_px).ascent;

    let shaper =
        SimpleShaper::with_properties(font.properties()).prepare(registry, font.instance().font());

    let mut y = bounds.origin.y;
    let mut error = None;
    let mut shaped_glyphs = 0u64;

    for_each_visible_text_line_with_boundaries(
        text,
        style.wrap,
        bounds.width(),
        style.max_lines,
        style.overflow,
        |line, from| shaper.next_cluster_boundary(registry, size_px, line, from),
        |line| line_width(&shaper, registry, size_px, line, ""),
        |line| line_width(&shaper, registry, size_px, line, ELLIPSIS),
        |line| {
            if error.is_some() {
                return;
            }

            let width = match line.width {
                Ok(width) => width,
                Err(shape_error) => {
                    error = Some(PaintError::Shape(shape_error));
                    return;
                }
            };

            let mut pen_x =
                aligned_line_x(bounds, width, style.align, paragraph_direction(line.text));
            let baseline = y + baseline_offset;
            let suffix = if line.ellipsis { ELLIPSIS } else { "" };
            let mut painted = Ok(());

            let shaped = shaper.shape_line_with(registry, size_px, line.text, suffix, |glyph| {
                if painted.is_ok() {
                    painted = paint_glyph(
                        painter,
                        registry,
                        glyph_cache,
                        size_px,
                        glyph,
                        baseline,
                        style,
                        clip,
                        &mut pen_x,
                        WordSpacing::NONE,
                    );
                }
            });

            let summary = match (shaped, painted) {
                (Ok(summary), Ok(())) => summary,
                (Err(shape_error), _) => {
                    error = Some(PaintError::Shape(shape_error));
                    return;
                }
                (_, Err(paint_error)) => {
                    error = Some(paint_error);
                    return;
                }
            };

            debug_assert_eq!(summary.advance(), width);

            shaped_glyphs = shaped_glyphs.saturating_add(summary.glyph_count() as u64);

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

    let size_px = font_size_px(style);

    let baseline_offset = font.metrics(size_px).ascent;
    let baseline = bounds.origin.y + baseline_offset;

    let shaper =
        SimpleShaper::with_properties(font.properties()).prepare(registry, font.instance().font());

    // this run was already positioned by its caller. Do not perform another
    // alignment/layout pass here.
    let mut pen_x = bounds.origin.x;
    let word_spacing = WordSpacing::new(text, style.word_spacing);
    let mut painted = Ok(());

    let summary = shaper
        .shape_line_with_pair_positioning_cache(
            registry,
            size_px,
            text,
            "",
            pair_positioning_cache,
            |glyph| {
                if painted.is_ok() {
                    painted = paint_glyph(
                        painter,
                        registry,
                        glyph_cache,
                        size_px,
                        glyph,
                        baseline,
                        style,
                        clip,
                        &mut pen_x,
                        word_spacing,
                    );
                }
            },
        )
        .map_err(PaintError::Shape)?;
    painted?;

    if style.underline
        && let Some(underline) = underline_rect(bounds.origin.x, pen_x, baseline, size_px)
    {
        painter
            .fill_rect(underline, style.color, clip)
            .map_err(PaintError::Painter)?;
    }

    Ok(u64::try_from(summary.glyph_count()).unwrap_or(u64::MAX))
}

#[allow(clippy::too_many_arguments)]
fn paint_glyph<P, const FONTS: usize, const GLYPH_SLOTS: usize, const GLYPH_BYTES: usize>(
    painter: &mut P,
    registry: &FontRegistry<'_, FONTS>,
    glyph_cache: &mut GlyphCache<GLYPH_SLOTS, GLYPH_BYTES>,
    size_px: u16,
    shaped: ShapedGlyph,
    baseline: Pixels,
    style: ResolvedTextStyle,
    clip: Rect,
    pen_x: &mut Pixels,
    word_spacing: WordSpacing<'_>,
) -> Result<(), PaintError<P::Error>>
where
    P: Painter,
{
    let bitmap = glyph_cache
        .get_or_rasterize(registry, shaped.font_instance(), shaped.glyph(), size_px)
        .map_err(PaintError::Font)?
        .with_darkness(style.text_darkness)
        .with_antialias(style.text_antialias);

    let metrics = bitmap.metrics();
    let offset = shaped.offset();

    let origin = Point::new(
        *pen_x + offset.x + metrics.bearing_x,
        baseline + offset.y + metrics.bearing_y,
    );

    painter
        .draw_glyph(&bitmap, origin, style.color, clip)
        .map_err(PaintError::Painter)?;

    *pen_x += shaped.advance() + word_spacing.after(shaped);

    Ok(())
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
