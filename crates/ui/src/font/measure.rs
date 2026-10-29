use crate::{
    FontRegistry, FontResources, LineHeight, Pixels, PreparedSimpleShaper, ResolvedFont,
    ResolvedTextStyle, SimpleShaper, Size, TextMeasurer, px,
    text_layout::{ELLIPSIS, LineWidth, for_each_visible_text_line_with_boundaries},
};

impl<const FONTS: usize, const GLYPH_SLOTS: usize, const GLYPH_BYTES: usize> TextMeasurer
    for FontResources<'_, FONTS, GLYPH_SLOTS, GLYPH_BYTES>
{
    fn measure_text(&self, text: &str, style: ResolvedTextStyle, max_size: Size) -> Size {
        if text.is_empty() {
            return Size::ZERO;
        }

        let font = self
            .resolve_family_font(style.font_family, style.font_weight, style.font_style)
            .expect("text measurement requires a default font");

        let registry = self.registry();
        let shaper = SimpleShaper::with_properties(font.properties())
            .prepare(registry, font.instance().font());

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
            |line, from| shaper.next_cluster_boundary(registry, size_px, line, from),
            |line| line_width(&shaper, registry, size_px, line, ""),
            |line| line_width(&shaper, registry, size_px, line, ELLIPSIS),
            |line| {
                // a line the shaper can't shape can't be painted either. It takes the
                // whole width, so the layout around it still leaves room for it
                longest_line = longest_line.max(line.width.unwrap_or(max_size.width));
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

/// the width of `text` followed by `suffix`, such as an ellipsis, on one line
pub(crate) fn line_width<'font, const FONTS: usize>(
    shaper: &PreparedSimpleShaper<'font>,
    registry: &'font FontRegistry<'_, FONTS>,
    size_px: u16,
    text: &str,
    suffix: &str,
) -> LineWidth {
    shaper
        .shape_line_with(registry, size_px, text, suffix, |_| {})
        .map(|summary| summary.advance())
}

pub(crate) fn text_line_advance(
    font: ResolvedFont<'_>,
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
