use ttf_parser::OutlineBuilder;

use super::metrics::{ceil_to_i32, floor_to_i32};

const SUPERSAMPLE_X: usize = 4;
pub(super) const SUPERSAMPLE_Y: usize = 4;
const COVERAGE_SAMPLES: u16 = (SUPERSAMPLE_X * SUPERSAMPLE_Y) as u16;

/// this is intentionally bounded
///
/// we don't allocate an edge/intersection vector while rasterizing. If a particular
/// outline exceeds the capacity, rasterization fails explecitly instead of silently
/// consuming heap memory.
///
/// Arabic/Latin glyphs are normally below this. If real fonts later demonstrate that
/// the value is too small, we can measure and tune it
const MAX_SCANLINE_INTERSECTIONS: usize = 96;

pub(super) const MAX_CURVE_STEPS: usize = 32;
const CURVE_PIXELS_PER_STEP: f32 = 2.0;

#[derive(Clone, Copy)]
pub(super) struct RasterPoint {
    pub(super) x: f32,
    pub(super) y: f32,
}

impl RasterPoint {
    pub(super) const ZERO: Self = Self { x: 0.0, y: 0.0 };
}

#[derive(Clone, Copy)]
pub(super) struct Intersection {
    pub(super) x: f32,
    pub(super) winding: i8,
}

impl Intersection {
    const EMPTY: Self = Self { x: 0.0, winding: 0 };
}

pub(super) struct ScanlineBuilder {
    scale: f32,
    left: f32,
    top: f32,
    sample_y: f32,

    current: Option<RasterPoint>,
    contour_start: Option<RasterPoint>,
    intersections: [Intersection; MAX_SCANLINE_INTERSECTIONS],

    len: usize,
    overflowed: bool,
}

impl ScanlineBuilder {
    pub(super) fn new(scale: f32, left: i32, top: i32, sample_y: f32) -> Self {
        Self {
            scale,
            left: left as f32,
            top: top as f32,
            sample_y,
            current: None,
            contour_start: None,
            intersections: [Intersection::EMPTY; MAX_SCANLINE_INTERSECTIONS],
            len: 0,
            overflowed: false,
        }
    }

    fn transform(&self, x: f32, y: f32) -> RasterPoint {
        RasterPoint {
            x: x * self.scale - self.left,
            y: -y * self.scale - self.top,
        }
    }

    fn push_intersection(&mut self, intersection: Intersection) {
        if self.len >= MAX_SCANLINE_INTERSECTIONS {
            self.overflowed = true;
            return;
        }

        self.intersections[self.len] = intersection;
        self.len += 1;
    }

    pub(super) fn add_segment(&mut self, from: RasterPoint, to: RasterPoint) {
        let winding = if from.y <= self.sample_y && self.sample_y < to.y {
            1
        } else if to.y <= self.sample_y && self.sample_y < from.y {
            -1
        } else {
            return;
        };

        let denominator = to.y - from.y;
        if denominator == 0.0 {
            return;
        }

        let t = (self.sample_y - from.y) / denominator;
        let x = from.x + (to.x - from.x) * t;

        self.push_intersection(Intersection { x, winding });
    }

    pub(super) fn sort_intersections(&mut self) {
        // intersection counts are intentionally tiny and bounded, so an allocation-free
        // insertion sort is appropriate here.
        let mut index = 1usize;

        while index < self.len {
            let value = self.intersections[index];
            let mut cursor = index;

            while cursor > 0 && self.intersections[cursor - 1].x > value.x {
                self.intersections[cursor] = self.intersections[cursor - 1];
                cursor -= 1;
            }

            self.intersections[cursor] = value;
            index += 1;
        }
    }

    pub(super) fn intersections(&self) -> &[Intersection] {
        &self.intersections[..self.len]
    }

    pub(super) const fn overflowed(&self) -> bool {
        self.overflowed
    }
}

impl OutlineBuilder for ScanlineBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        let point = self.transform(x, y);

        self.current = Some(point);
        self.contour_start = Some(point);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let to = self.transform(x, y);

        if let Some(from) = self.current {
            self.add_segment(from, to);
        }

        self.current = Some(to);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let end = self.transform(x, y);

        let Some(start) = self.current else {
            self.current = Some(end);

            return;
        };

        let control = self.transform(x1, y1);
        let steps = curve_steps(&[start, control, end]);
        let mut previous = start;

        for step in 1..=steps {
            let t = step as f32 / steps as f32;
            let inverse = 1.0 - t;
            let point = RasterPoint {
                x: inverse * inverse * start.x + 2.0 * inverse * t * control.x + t * t * end.x,
                y: inverse * inverse * start.y + 2.0 * inverse * t * control.y + t * t * end.y,
            };

            self.add_segment(previous, point);

            previous = point;
        }

        self.current = Some(end);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let end = self.transform(x, y);

        let Some(start) = self.current else {
            self.current = Some(end);

            return;
        };

        let control_1 = self.transform(x1, y1);
        let control_2 = self.transform(x2, y2);
        let steps = curve_steps(&[start, control_1, control_2, end]);
        let mut previous = start;

        for step in 1..=steps {
            let t = step as f32 / steps as f32;
            let inverse = 1.0 - t;
            let inverse_2 = inverse * inverse;
            let t_2 = t * t;

            let point = RasterPoint {
                x: inverse_2 * inverse * start.x
                    + 3.0 * inverse_2 * t * control_1.x
                    + 3.0 * inverse * t_2 * control_2.x
                    + t_2 * t * end.x,
                y: inverse_2 * inverse * start.y
                    + 3.0 * inverse_2 * t * control_1.y
                    + 3.0 * inverse * t_2 * control_2.y
                    + t_2 * t * end.y,
            };

            self.add_segment(previous, point);

            previous = point;
        }

        self.current = Some(end);
    }

    fn close(&mut self) {
        if let (Some(current), Some(start)) = (self.current, self.contour_start) {
            self.add_segment(current, start);
        }

        self.current = None;
        self.contour_start = None;
    }
}

pub(super) fn curve_steps(points: &[RasterPoint]) -> usize {
    if points.len() < 2 {
        return 1;
    }

    let mut length = 0.0f32;

    for pair in points.windows(2) {
        length += approximate_distance(pair[0], pair[1]);
    }

    let estimate = length / CURVE_PIXELS_PER_STEP;
    let truncated = estimate as usize;

    let mut steps = if (truncated as f32) < estimate {
        truncated.saturating_add(1)
    } else {
        truncated
    };

    if steps == 0 {
        steps = 1;
    }

    steps.min(MAX_CURVE_STEPS)
}

fn approximate_distance(from: RasterPoint, to: RasterPoint) -> f32 {
    abs_f32(to.x - from.x) + abs_f32(to.y - from.y)
}

pub(super) fn accumulate_scanline(
    width: usize,
    row: usize,
    coverage: &mut [u8],
    intersections: &[Intersection],
) {
    if intersections.is_empty() {
        return;
    }

    let mut index = 0usize;
    let mut winding = 0i16;
    let mut previous_x = intersections[0].x;

    while index < intersections.len() {
        let x = intersections[index].x;
        if winding != 0 {
            accumulate_interval(width, row, coverage, previous_x, x);
        }

        let mut delta = 0i16;

        while index < intersections.len() && intersections[index].x == x {
            delta += i16::from(intersections[index].winding);
            index += 1;
        }

        winding += delta;
        previous_x = x;
    }
}

fn accumulate_interval(width: usize, row: usize, coverage: &mut [u8], start: f32, end: f32) {
    if width == 0 || end <= start {
        return;
    }

    let start = if start < 0.0 { 0.0 } else { start };
    let max_x = width as f32;
    let end = if end > max_x { max_x } else { end };
    if end <= start {
        return;
    }

    let first_pixel = floor_to_i32(start).max(0);
    let last_pixel = ceil_to_i32(end).min(i32::try_from(width).unwrap_or(i32::MAX));
    let Ok(first_pixel) = usize::try_from(first_pixel) else {
        return;
    };

    let Ok(last_pixel) = usize::try_from(last_pixel) else {
        return;
    };

    for x in first_pixel..last_pixel {
        let Some(pixel) = row
            .checked_mul(width)
            .and_then(|row_start| row_start.checked_add(x))
            .and_then(|index| coverage.get_mut(index))
        else {
            continue;
        };

        for sub_x in 0..SUPERSAMPLE_X {
            let sample_x = x as f32 + (sub_x as f32 + 0.5) / SUPERSAMPLE_X as f32;
            if sample_x >= start && sample_x < end {
                *pixel = pixel.saturating_add(1);
            }
        }
    }
}

pub(super) fn normalize_coverage(coverage: &mut [u8]) {
    for value in coverage {
        let samples = u16::from(*value);

        *value = (samples
            .saturating_mul(255)
            .saturating_add(COVERAGE_SAMPLES / 2)
            / COVERAGE_SAMPLES) as u8;
    }
}




fn abs_f32(value: f32) -> f32 {
    if value < 0.0 { -value } else { value }
}
