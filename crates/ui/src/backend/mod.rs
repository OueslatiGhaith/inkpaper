#[cfg(feature = "eink")]
mod eink;
#[cfg(feature = "embedded-graphics")]
mod embedded_graphics;
mod mono_font;

#[cfg(feature = "eink")]
pub use eink::{
    DEFAULT_MIN_INK_COVERAGE, EInkCoverageBitmap, EInkCoverageBlitter, EInkCoverageMode, EInkError,
    EInkPaintReport, EInkPainter, EInkTone, EInkUiMode, Gray2, inks,
};

#[cfg(feature = "embedded-graphics")]
pub use embedded_graphics::{
    CoverageMode, EmbeddedGraphicsError, EmbeddedGraphicsImage, EmbeddedGraphicsPainter,
};

pub use mono_font::MonoFontFace;

/// extra advance after each U+0020 space of a run's source text
#[cfg(any(feature = "eink", feature = "embedded-graphics"))]
#[derive(Debug, Clone, Copy)]
struct WordSpacing<'a> {
    text: &'a str,
    extra: crate::Pixels,
}

#[cfg(any(feature = "eink", feature = "embedded-graphics"))]
impl<'a> WordSpacing<'a> {
    const NONE: Self = Self::new("", crate::Pixels::ZERO);

    const fn new(text: &'a str, extra: crate::Pixels) -> Self {
        Self { text, extra }
    }

    fn after(self, glyph: crate::ShapedGlyph) -> crate::Pixels {
        if self.text.as_bytes().get(glyph.cluster()) == Some(&b' ') {
            self.extra
        } else {
            crate::Pixels::ZERO
        }
    }
}

/// A text run's underline, from `start` to `end` just below the baseline like
/// crosspoint's, thickening with the font size.
#[cfg(any(feature = "eink", feature = "embedded-graphics"))]
fn underline_rect(
    start: crate::Pixels,
    end: crate::Pixels,
    baseline: crate::Pixels,
    size_px: u16,
) -> Option<crate::Rect> {
    let width = end - start;

    if width.is_non_positive() {
        return None;
    }

    let thickness = crate::px(i32::from((size_px / 20).max(1)));

    Some(crate::Rect::new(
        crate::Point::new(start, baseline + crate::px(2)),
        crate::Size::new(width, thickness),
    ))
}
