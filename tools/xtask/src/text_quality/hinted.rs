//! glyph coverage from skrifa outlines, to measure hinting before the device has it.
//!
//! outlines are filled like `crates/ui/src/font/ttf/raster.rs` fills them: curves
//! flattened to at most 2px steps, 4x4 samples per pixel, nonzero winding. With
//! `--hinting none` the harness should therefore measure what the device draws.

use anyhow::{Result, anyhow};
use inkpaper_ui::GlyphId;
use skrifa::{
    FontRef, MetadataProvider,
    instance::{Location, Size},
    outline::{
        DrawSettings, Engine, HintingInstance, HintingOptions, OutlineGlyphCollection, OutlinePen,
        SmoothMode, Target,
    },
};

const SUPERSAMPLE: usize = 4;
const MAX_CURVE_STEPS: usize = 32;
const CURVE_PIXELS_PER_STEP: f32 = 2.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Hinting {
    /// skrifa's unhinted outlines, the control for the other modes
    None,
    /// FreeType's light autohinting: vertical snapping only
    Light,
    /// FreeType's normal autohinting: vertical and horizontal snapping
    Normal,
    /// FreeType's autohinting for monochrome output
    Mono,
}

pub struct GlyphCoverage {
    pub left: i32,
    pub top: i32,
    pub width: usize,
    pub height: usize,
    pub coverage: Vec<u8>,
}

pub struct HintedFont {
    outlines: OutlineGlyphCollection<'static>,
    size: f32,
    location: Location,
    instance: Option<HintingInstance>,
}

impl HintedFont {
    pub fn new(data: &'static [u8], size: u16, weight: f32, hinting: Hinting) -> Result<Self> {
        let font = FontRef::new(data).map_err(|error| anyhow!("{error}"))?;
        let outlines = font.outline_glyphs();
        let location = font.axes().location([("wght", weight)]);

        let target = match hinting {
            Hinting::None => None,
            // skrifa treats preserved linear metrics as light hinting
            Hinting::Light => Some(smooth(SmoothMode::Light, true)),
            Hinting::Normal => Some(smooth(SmoothMode::Normal, false)),
            Hinting::Mono => Some(Target::Mono),
        };

        let instance = target
            .map(|target| {
                HintingInstance::new(
                    &outlines,
                    Size::new(f32::from(size)),
                    &location,
                    HintingOptions {
                        engine: Engine::Auto(None),
                        target,
                    },
                )
                .map_err(|error| anyhow!("{error}"))
            })
            .transpose()?;

        Ok(Self {
            outlines,
            size: f32::from(size),
            location,
            instance,
        })
    }

    pub fn glyph(&self, glyph: GlyphId) -> Result<GlyphCoverage> {
        let outline = self
            .outlines
            .get(skrifa::GlyphId::new(u32::from(glyph.value())))
            .ok_or_else(|| anyhow!("no outline for glyph {}", glyph.value()))?;

        let settings = match &self.instance {
            Some(instance) => DrawSettings::hinted(instance, false),
            None => DrawSettings::unhinted(Size::new(self.size), &self.location),
        };

        let mut pen = FlatteningPen::default();

        outline
            .draw(settings, &mut pen)
            .map_err(|error| anyhow!("{error}"))?;

        Ok(fill(&pen.segments))
    }
}

fn smooth(mode: SmoothMode, preserve_linear_metrics: bool) -> Target {
    Target::Smooth {
        mode,
        symmetric_rendering: true,
        preserve_linear_metrics,
    }
}

#[derive(Clone, Copy)]
struct PenPoint {
    x: f32,
    y: f32,
}

/// collects an outline as line segments in pixels, y down
#[derive(Default)]
struct FlatteningPen {
    segments: Vec<(PenPoint, PenPoint)>,
    current: Option<PenPoint>,
    contour_start: Option<PenPoint>,
}

impl FlatteningPen {
    fn point(x: f32, y: f32) -> PenPoint {
        PenPoint { x, y: -y }
    }

    fn curve(&mut self, controls: &[PenPoint], at: impl Fn(f32) -> PenPoint) {
        let Some(start) = self.current else {
            self.current = controls.last().copied();
            return;
        };

        let mut points = vec![start];
        points.extend_from_slice(controls);

        let steps = curve_steps(&points);
        let mut previous = start;

        for step in 1..=steps {
            let point = at(step as f32 / steps as f32);

            self.segments.push((previous, point));
            previous = point;
        }

        self.current = controls.last().copied();
    }
}

impl OutlinePen for FlatteningPen {
    fn move_to(&mut self, x: f32, y: f32) {
        let point = Self::point(x, y);

        self.current = Some(point);
        self.contour_start = Some(point);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let to = Self::point(x, y);

        if let Some(from) = self.current {
            self.segments.push((from, to));
        }

        self.current = Some(to);
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        let control = Self::point(cx0, cy0);
        let end = Self::point(x, y);
        let start = self.current.unwrap_or(end);

        self.curve(&[control, end], |t| {
            let inverse = 1.0 - t;

            PenPoint {
                x: inverse * inverse * start.x + 2.0 * inverse * t * control.x + t * t * end.x,
                y: inverse * inverse * start.y + 2.0 * inverse * t * control.y + t * t * end.y,
            }
        });
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        let first = Self::point(cx0, cy0);
        let second = Self::point(cx1, cy1);
        let end = Self::point(x, y);
        let start = self.current.unwrap_or(end);

        self.curve(&[first, second, end], |t| {
            let inverse = 1.0 - t;
            let cubic = |a: f32, b: f32, c: f32, d: f32| {
                inverse * inverse * inverse * a
                    + 3.0 * inverse * inverse * t * b
                    + 3.0 * inverse * t * t * c
                    + t * t * t * d
            };

            PenPoint {
                x: cubic(start.x, first.x, second.x, end.x),
                y: cubic(start.y, first.y, second.y, end.y),
            }
        });
    }

    fn close(&mut self) {
        if let (Some(current), Some(start)) = (self.current, self.contour_start) {
            self.segments.push((current, start));
        }

        self.current = None;
        self.contour_start = None;
    }
}

fn curve_steps(points: &[PenPoint]) -> usize {
    let length: f32 = points
        .windows(2)
        .map(|pair| (pair[1].x - pair[0].x).abs() + (pair[1].y - pair[0].y).abs())
        .sum();

    ((length / CURVE_PIXELS_PER_STEP).ceil() as usize).clamp(1, MAX_CURVE_STEPS)
}

fn fill(segments: &[(PenPoint, PenPoint)]) -> GlyphCoverage {
    let bounds = segments.iter().flat_map(|(from, to)| [from, to]).fold(
        None,
        |bounds: Option<(f32, f32, f32, f32)>, point| {
            Some(match bounds {
                None => (point.x, point.y, point.x, point.y),
                Some((left, top, right, bottom)) => (
                    left.min(point.x),
                    top.min(point.y),
                    right.max(point.x),
                    bottom.max(point.y),
                ),
            })
        },
    );

    let Some((left, top, right, bottom)) = bounds else {
        return GlyphCoverage {
            left: 0,
            top: 0,
            width: 0,
            height: 0,
            coverage: Vec::new(),
        };
    };

    let left = left.floor() as i32;
    let top = top.floor() as i32;
    let width = (right.ceil() as i32 - left).max(0) as usize;
    let height = (bottom.ceil() as i32 - top).max(0) as usize;

    let mut samples = vec![0u16; width * height];
    let mut crossings: Vec<(f32, i32)> = Vec::new();

    for row in 0..height {
        for sub_y in 0..SUPERSAMPLE {
            let sample_y = (top + row as i32) as f32 + (sub_y as f32 + 0.5) / SUPERSAMPLE as f32;

            crossings.clear();
            crossings.extend(segments.iter().filter_map(|(from, to)| {
                let winding = if from.y <= sample_y && sample_y < to.y {
                    1
                } else if to.y <= sample_y && sample_y < from.y {
                    -1
                } else {
                    return None;
                };

                let t = (sample_y - from.y) / (to.y - from.y);

                Some((from.x + (to.x - from.x) * t - left as f32, winding))
            }));
            crossings.sort_by(|a, b| a.0.total_cmp(&b.0));

            let mut winding = 0;

            for pair in crossings.windows(2) {
                winding += pair[0].1;

                if winding != 0 {
                    add_interval(&mut samples[row * width..][..width], pair[0].0, pair[1].0);
                }
            }
        }
    }

    let full = (SUPERSAMPLE * SUPERSAMPLE) as u16;

    GlyphCoverage {
        left,
        top,
        width,
        height,
        coverage: samples
            .into_iter()
            .map(|count| ((count * 255 + full / 2) / full) as u8)
            .collect(),
    }
}

fn add_interval(row: &mut [u16], start: f32, end: f32) {
    let start = start.max(0.0);
    let end = end.min(row.len() as f32);

    if end <= start {
        return;
    }

    for x in start.floor() as usize..(end.ceil() as usize).min(row.len()) {
        for sub_x in 0..SUPERSAMPLE {
            let sample_x = x as f32 + (sub_x as f32 + 0.5) / SUPERSAMPLE as f32;

            if sample_x >= start && sample_x < end {
                row[x] += 1;
            }
        }
    }
}
