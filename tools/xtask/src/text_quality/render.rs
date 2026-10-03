//! renders the corpus through the path the reader uses on the device:
//! runtime canvas -> `PaintCx::draw_text_run` -> `EInkPainter` in `BinaryDither` mode.
//!
//! like the reader, each line is wrapped from separately measured words and drawn as
//! one run.
//!
//! the firmware also installs a fast coverage blitter. its output is pinned to
//! this generic path by `firmware/x4-pro/tests/coverage_blitter.rs`.
//!
//! the metrics read glyphs from placements recorded alongside, so every run checks
//! that compositing those placements reproduces the painted page.

use std::convert::Infallible;

use anyhow::{Result, anyhow, bail, ensure};
use embedded_graphics::{
    Pixel,
    pixelcolor::GrayColor,
    prelude::{Dimensions, DrawTarget, Point as EgPoint, Size as EgSize},
    primitives::Rectangle,
};
use inkpaper_ui::{
    FontData, FontFace, FontWeight, GlyphId, HintedTtfFont, ResolvedFont, RuntimeResources,
    ShapedGlyph, SimpleShaper, TtfFont,
    backend::{DEFAULT_MIN_INK_COVERAGE, EInkPainter, EInkUiMode, Gray2, inks},
    prelude::*,
};

/// the reader's font. Keep in sync with `crates/app/src/typography.rs`
static READER_FONT: TtfFont<'static> = TtfFont::from_data(
    FontData::new(include_bytes!(
        "../../../../crates/app/assets/fonts/InterVariable.ttf"
    )),
    0,
);

static HINTED_READER_FONT: HintedTtfFont<'static> = HintedTtfFont::new(READER_FONT);

pub const PAGE_WIDTH: usize = 480;

// the reader page column: see `screens/reader.rs`
const COLUMN_X: i32 = 20;
const COLUMN_WIDTH: i32 = 440;
const TOP: i32 = 20;
const BOTTOM: i32 = 20;

/// ideal pen positions come from shaping at this multiple of the size, which leaves
/// at most 1/128 px of rounding per glyph
const IDEAL_POSITION_SCALE: u16 = 64;

/// the ideal image is rasterized at this multiple of the size and box-filtered down
const IDEAL_RASTER_SCALE: u16 = 4;

const SHAPED_GLYPHS: usize = 256;

type MeasureResources = RuntimeResources<'static, 1, 1024, { 4 << 20 }, 0>;

#[derive(Clone, Copy)]
pub struct ClipRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl ClipRect {
    pub fn contains(self, x: i32, y: i32) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }
}

/// one glyph occurrence, positioned exactly as `EInkPainter` positions it
pub struct Placement {
    pub glyph: GlyphId,
    /// top-left of the coverage bitmap
    pub x: i32,
    pub y: i32,
    pub width: usize,
    pub height: usize,
    pub coverage: Vec<u8>,
    pub baseline: i32,
    pub pen_x: i32,
    pub ideal_pen_x: f64,
    pub first_in_line: bool,
    pub clip: ClipRect,
}

impl Placement {
    pub fn covers(&self, dx: usize, dy: usize) -> bool {
        self.coverage[dy * self.width + dx] > 0
    }
}

pub struct Rendered {
    pub width: usize,
    pub height: usize,
    /// device output, true = black
    pub ink: Vec<bool>,
    /// outline coverage in 0..=1, rendered at 4x with ideal positions
    pub ideal: Vec<f32>,
    pub glyphs: Vec<Placement>,
    pub text_area: ClipRect,
    pub x_height: i32,
    /// glyph ids of letters whose middle row crosses only straight stems, with their
    /// stem count
    pub stem_glyphs: Vec<(GlyphId, usize)>,
}

pub fn render(
    corpus: &str,
    size: u16,
    min_ink_coverage: Option<u8>,
    hinted: bool,
) -> Result<Rendered> {
    let drawn_font: &'static dyn FontFace = if hinted {
        &HINTED_READER_FONT
    } else {
        &READER_FONT
    };

    // shaping and the ideal image always use the unhinted outlines
    let (mut resources, font) = font_resources(&READER_FONT)?;
    let (mut glyph_resources, glyph_font) = font_resources(drawn_font)?;

    let metrics = font.metrics(size);
    let line_height = metrics.line_height().get();
    let ascent = metrics.ascent.get();

    let lines = wrap(corpus, |text| measure(&resources, font, size, text))?;

    let width = PAGE_WIDTH;
    let height = usize::try_from(TOP + line_height * lines.len() as i32 + BOTTOM)?;

    let mut glyphs = Vec::new();
    let mut ideal = vec![0f32; width * height];
    let mut runs = Vec::new();

    for (index, line) in lines.iter().enumerate() {
        let top = TOP + line_height * index as i32;

        let clip = ClipRect {
            left: COLUMN_X,
            top,
            right: COLUMN_X + COLUMN_WIDTH,
            bottom: top + line_height,
        };

        place_line(
            &mut resources,
            &mut glyph_resources,
            font,
            glyph_font,
            size,
            line,
            top + ascent,
            clip,
            &mut glyphs,
            &mut ideal,
            width,
        )?;

        runs.push((line.clone(), clip));
    }

    let min_ink_coverage = min_ink_coverage.unwrap_or(DEFAULT_MIN_INK_COVERAGE);
    let composited = composite(&glyphs, width, height, min_ink_coverage);

    let ink = paint_page(runs, size, width, height, min_ink_coverage, drawn_font)?;

    ensure!(
        ink == composited,
        "compositing the placements does not reproduce the painted page"
    );

    let x_glyph = glyph_id(font, 'x')?;
    let x_height = -glyph_resources
        .glyph_bitmap(glyph_font.instance(), x_glyph, size)
        .map_err(|error| anyhow!("{error:?}"))?
        .metrics()
        .bearing_y
        .get();

    let stem_glyphs = [
        ('n', 2),
        ('m', 3),
        ('h', 2),
        ('u', 2),
        ('l', 1),
        ('i', 1),
        ('r', 1),
    ]
    .into_iter()
    .map(|(character, stems)| Ok((glyph_id(font, character)?, stems)))
    .collect::<Result<_>>()?;

    for value in &mut ideal {
        *value = value.min(1.0);
    }

    Ok(Rendered {
        width,
        height,
        ink,
        ideal,
        glyphs,
        text_area: ClipRect {
            left: COLUMN_X,
            top: TOP,
            right: COLUMN_X + COLUMN_WIDTH,
            bottom: height as i32 - BOTTOM,
        },
        x_height,
        stem_glyphs,
    })
}

fn glyph_id(font: ResolvedFont<'_>, character: char) -> Result<GlyphId> {
    font.face()
        .glyph_id(character)
        .ok_or_else(|| anyhow!("reader font has no glyph for {character:?}"))
}

fn measure(
    resources: &MeasureResources,
    font: ResolvedFont<'_>,
    size: u16,
    text: &str,
) -> Result<i32> {
    let registry = resources.font_registry();
    let shaper = SimpleShaper::with_properties(font.properties());
    let mut output = [ShapedGlyph::EMPTY; SHAPED_GLYPHS];

    let summary = shaper
        .measure(&registry, font.id(), size, text, &mut output)
        .map_err(|error| anyhow!("{error:?}"))?;

    Ok(summary.advance().get())
}

/// greedy word wrap into the reader column, one paragraph per corpus line.
///
/// like the reader's pagination, words and spaces are measured separately
fn wrap(corpus: &str, mut measure: impl FnMut(&str) -> Result<i32>) -> Result<Vec<String>> {
    let space = measure(" ")?;
    let mut lines = Vec::new();

    for paragraph in corpus.lines().filter(|line| !line.trim().is_empty()) {
        let mut line = String::new();
        let mut line_width = 0;

        for word in paragraph.split_whitespace() {
            let width = measure(word)?;

            if line.is_empty() {
                line_width = width;
            } else if line_width + space + width > COLUMN_WIDTH {
                lines.push(std::mem::take(&mut line));
                line_width = width;
            } else {
                line.push(' ');
                line_width += space + width;
            }

            line.push_str(word);
        }

        if !line.is_empty() {
            lines.push(line);
        }
    }

    Ok(lines)
}

fn font_resources(
    face: &'static dyn FontFace,
) -> Result<(Box<MeasureResources>, ResolvedFont<'static>)> {
    let mut resources = Box::new(MeasureResources::default());
    let family = resources
        .register_font_family()
        .map_err(|error| anyhow!("{error:?}"))?;
    resources
        .register_font_face(family, face)
        .map_err(|error| anyhow!("{error:?}"))?;

    let font = resources
        .resolve_font_family_weight(family, FontWeight::NORMAL)
        .ok_or_else(|| anyhow!("reader font did not resolve"))?;

    Ok((resources, font))
}

/// the page `EInkPainter` draws for black text on white in `BinaryDither` mode
fn composite(glyphs: &[Placement], width: usize, height: usize, min_coverage: u8) -> Vec<bool> {
    let mut ink = vec![false; width * height];

    for glyph in glyphs {
        for (index, coverage) in glyph.coverage.iter().enumerate() {
            let x = glyph.x + (index % glyph.width) as i32;
            let y = glyph.y + (index / glyph.width) as i32;

            if inks(*coverage, min_coverage)
                && glyph.clip.contains(x, y)
                && (x as usize) < width
                && (y as usize) < height
            {
                ink[y as usize * width + x as usize] = true;
            }
        }
    }

    ink
}

#[allow(clippy::too_many_arguments)]
fn place_line(
    resources: &mut MeasureResources,
    glyph_resources: &mut MeasureResources,
    font: ResolvedFont<'_>,
    glyph_font: ResolvedFont<'_>,
    size: u16,
    text: &str,
    baseline: i32,
    clip: ClipRect,
    glyphs: &mut Vec<Placement>,
    ideal: &mut [f32],
    page_width: usize,
) -> Result<()> {
    let registry = resources.font_registry();
    let shaper = SimpleShaper::with_properties(font.properties());

    let mut actual = [ShapedGlyph::EMPTY; SHAPED_GLYPHS];
    let actual = shaper
        .shape_into(&registry, font.id(), size, text, &mut actual)
        .map_err(|error| anyhow!("{error:?}"))?
        .glyphs()
        .to_vec();

    let mut scaled = [ShapedGlyph::EMPTY; SHAPED_GLYPHS];
    let scaled = shaper
        .shape_into(
            &registry,
            font.id(),
            size * IDEAL_POSITION_SCALE,
            text,
            &mut scaled,
        )
        .map_err(|error| anyhow!("{error:?}"))?
        .glyphs()
        .to_vec();

    ensure!(
        actual.len() == scaled.len()
            && actual
                .iter()
                .zip(&scaled)
                .all(|(actual, scaled)| actual.glyph() == scaled.glyph()),
        "shaping {text:?} gave different glyphs at {size}px and {}px",
        size * IDEAL_POSITION_SCALE
    );

    let position_scale = f64::from(IDEAL_POSITION_SCALE);
    let raster_scale = i32::from(IDEAL_RASTER_SCALE);
    let page_height = ideal.len() / page_width;

    let mut pen_x = clip.left;
    let mut ideal_pen_x = f64::from(clip.left);

    for (index, (shaped, scaled)) in actual.iter().zip(&scaled).enumerate() {
        // mirrors draw_shaped_run in crates/ui/src/backend/eink/text.rs
        let bitmap = glyph_resources
            .glyph_bitmap(glyph_font.instance(), shaped.glyph(), size)
            .map_err(|error| anyhow!("{error:?}"))?;
        let metrics = bitmap.metrics();
        let offset = shaped.offset();

        glyphs.push(Placement {
            glyph: shaped.glyph(),
            x: pen_x + offset.x.get() + metrics.bearing_x.get(),
            y: baseline + offset.y.get() + metrics.bearing_y.get(),
            width: usize::from(metrics.width),
            height: usize::from(metrics.height),
            coverage: bitmap.coverage().to_vec(),
            baseline,
            pen_x,
            ideal_pen_x,
            first_in_line: index == 0,
            clip,
        });

        let ideal_x = ideal_pen_x + f64::from(scaled.offset().x.get()) / position_scale;
        let ideal_y = f64::from(scaled.offset().y.get()) / position_scale;

        let large = resources
            .glyph_bitmap(
                shaped.font_instance(),
                shaped.glyph(),
                size * IDEAL_RASTER_SCALE,
            )
            .map_err(|error| anyhow!("{error:?}"))?;
        let large_metrics = large.metrics();
        let large_width = usize::from(large_metrics.width);

        let origin_x =
            (ideal_x * f64::from(raster_scale)).round() as i32 + large_metrics.bearing_x.get();
        let origin_y = baseline * raster_scale
            + (ideal_y * f64::from(raster_scale)).round() as i32
            + large_metrics.bearing_y.get();

        for (sample, coverage) in large.coverage().iter().enumerate() {
            if *coverage == 0 {
                continue;
            }

            let x = (origin_x + (sample % large_width) as i32).div_euclid(raster_scale);
            let y = (origin_y + (sample / large_width) as i32).div_euclid(raster_scale);

            if !clip.contains(x, y) || x as usize >= page_width || y as usize >= page_height {
                continue;
            }

            ideal[y as usize * page_width + x as usize] +=
                f32::from(*coverage) / 255.0 / (raster_scale * raster_scale) as f32;
        }

        pen_x += shaped.advance().get();
        ideal_pen_x += f64::from(scaled.advance().get()) / position_scale;
    }

    Ok(())
}

struct PageView {
    lines: Vec<(String, ClipRect)>,
    size: u16,
    width: i32,
    height: i32,
}

impl PageView {
    fn paint(&self, paint: &mut PaintCx<'_>) {
        for (line, clip) in &self.lines {
            paint.draw_text_run(
                Rect::new(
                    Point::new(px(clip.left), px(clip.top)),
                    Size::new(px(clip.right - clip.left), px(clip.bottom - clip.top)),
                ),
                text(line.as_str())
                    .font_weight(FontWeight::NORMAL)
                    .font_size(px(i32::from(self.size))),
            );
        }
    }
}

impl Render for PageView {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        let page = cx.canvas(|view: &PageView, paint| view.paint(paint));

        div()
            .w(px(self.width))
            .h(px(self.height))
            .bg(Color::WHITE)
            .text_color(Color::BLACK)
            .child(page.size(Size::new(px(self.width), px(self.height))))
    }
}

fn paint_page(
    lines: Vec<(String, ClipRect)>,
    size: u16,
    width: usize,
    height: usize,
    min_ink_coverage: u8,
    font: &'static dyn FontFace,
) -> Result<Vec<bool>> {
    let mut runtime =
        Runtime::<HeapStorage, RuntimeResources<'_, 1, 256, { 256 * 1024 }, 0>>::default();

    let family = runtime
        .register_font_family()
        .map_err(|error| anyhow!("{error:?}"))?;
    runtime
        .register_font_face(family, font)
        .map_err(|error| anyhow!("{error:?}"))?;

    let view = PageView {
        lines,
        size,
        width: i32::try_from(width)?,
        height: i32::try_from(height)?,
    };

    runtime
        .create_root(|_| view)
        .map_err(|error| anyhow!("{error:?}"))?;
    runtime.rebuild().map_err(|error| anyhow!("{error:?}"))?;
    runtime.layout(Size::new(px(width as i32), px(height as i32)));

    let mut page = PageTarget::new(width, height);

    {
        let mut painter = EInkPainter::new(&mut page)
            .with_ui_mode(EInkUiMode::BinaryDither)
            .with_min_ink_coverage(min_ink_coverage);

        runtime
            .paint(&mut painter)
            .map_err(|error| anyhow!("{error:?}"))?
            .ok_or_else(|| anyhow!("runtime painted nothing"))?;
    }

    page.levels
        .iter()
        .map(|level| match level {
            0 => Ok(true),
            3 => Ok(false),
            level => bail!("the page contains gray level {level}; only binary output is measured"),
        })
        .collect()
}

struct PageTarget {
    width: usize,
    height: usize,
    levels: Vec<u8>,
}

impl PageTarget {
    fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            levels: vec![3; width * height],
        }
    }
}

impl Dimensions for PageTarget {
    fn bounding_box(&self) -> Rectangle {
        Rectangle::new(
            EgPoint::zero(),
            EgSize::new(self.width as u32, self.height as u32),
        )
    }
}

impl DrawTarget for PageTarget {
    type Color = Gray2;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(point, color) in pixels {
            let (Ok(x), Ok(y)) = (usize::try_from(point.x), usize::try_from(point.y)) else {
                continue;
            };

            if x < self.width && y < self.height {
                self.levels[y * self.width + x] = color.luma();
            }
        }

        Ok(())
    }
}
