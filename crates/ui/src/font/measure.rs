use crate::{
    FontFace, FontId, FontRegistry, FontResources, LineHeight, Pixels, ResolvedTextStyle,
    ShapeState, ShapedGlyph, SimpleShaper, Size, TextMeasurer, px,
    text_layout::{ELLIPSIS, for_each_visible_text_line_with_boundaries},
};

pub(crate) const SHAPED_LINE_GLYPH_CAPACITY: usize = 128;

impl<const FONTS: usize, const GLYPH_SLOTS: usize, const GLYPH_BYTES: usize> TextMeasurer
    for FontResources<'_, FONTS, GLYPH_SLOTS, GLYPH_BYTES>
{
    fn measure_text(&self, text: &str, style: ResolvedTextStyle, max_size: Size) -> Size {
        if text.is_empty() {
            return Size::ZERO;
        }

        let (font_id, font) = self
            .resolve(style.font)
            .expect("text measurement requires a default font");

        let registry = self.registry();
        let shaper = SimpleShaper::new();

        let size_px = font_size_px(style);
        let glyph_height = font.metrics(size_px).line_height();
        let line_advance = text_line_advance(font, size_px, style);

        let mut longest_line = Pixels::ZERO;
        let mut line_count = 0i32;

        for_each_visible_text_line_with_boundaries(
            text,
            style.wrap,
            max_size.width,
            style.max_lines,
            style.overflow,
            |line, from| shaper.next_cluster_boundary(&registry, font_id, size_px, line, from),
            |line| measure_shaped_line(&registry, font_id, size_px, line),
            |line| measure_shaped_line_with_ellipsis(&registry, font_id, size_px, line),
            |line| {
                longest_line = longest_line.max(line.width);
                line_count = line_count.saturating_add(1);
            },
        );

        if line_count == 0 {
            return Size::ZERO;
        }

        let height =
            glyph_height.saturating_add(line_advance.saturating_mul(line_count.saturating_sub(1)));

        Size::new(
            longest_line
                .non_negative()
                .min(max_size.width.non_negative()),
            height.non_negative().min(max_size.height.non_negative()),
        )
    }
}

pub(crate) fn measure_shaped_line<const FONTS: usize>(
    registry: &FontRegistry<'_, FONTS>,
    font: FontId,
    size_px: u16,
    text: &str,
) -> Pixels {
    let mut glyphs = [ShapedGlyph::EMPTY; SHAPED_LINE_GLYPH_CAPACITY];

    match SimpleShaper::new().measure(registry, font, size_px, text, &mut glyphs) {
        Ok(summary) => summary.advance(),
        Err(_) => Pixels::MAX,
    }
}

pub(crate) fn measure_shaped_line_with_ellipsis<const FONTS: usize>(
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

pub(crate) fn text_line_advance(
    font: &dyn FontFace,
    size_px: u16,
    style: ResolvedTextStyle,
) -> Pixels {
    match style.line_height {
        LineHeight::Normal => font.metrics(size_px).line_height().non_negative(),

        LineHeight::Pixels(height) => height.non_negative(),
    }
}

pub(crate) fn font_size_px(style: ResolvedTextStyle) -> u16 {
    let size = style.font_size.max(px(1)).get();

    u16::try_from(size).unwrap_or(u16::MAX)
}
