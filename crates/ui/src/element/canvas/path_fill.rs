use crate::{
    AffineTransform, CanvasPainter, Color, FillRule, PathFill, Point, Rect, Size, VectorPath,
    VectorPoint, element::canvas::path::floor_to_i32, px,
};

use super::path::{FlattenedPathSink, ceil_to_i32, flatten_path};

const MAX_INTERSECTIONS: usize = 96;

#[derive(Clone, Copy)]
struct Intersection {
    x: f32,
    winding: i8,
}

impl Intersection {
    const EMPTY: Self = Self { x: 0.0, winding: 0 };
}

#[derive(Default)]
struct BoundsSink {
    min_x: f32,
    max_x: f32,
    min_y: f32,
    max_y: f32,
    initialized: bool,
}

impl BoundsSink {
    fn include(&mut self, point: VectorPoint) {
        if !self.initialized {
            self.min_x = point.x;
            self.max_x = point.x;
            self.min_y = point.y;
            self.max_y = point.y;
            self.initialized = true;
            return;
        }

        self.min_x = self.min_x.min(point.x);
        self.max_x = self.max_x.max(point.x);
        self.min_y = self.min_y.min(point.y);
        self.max_y = self.max_y.max(point.y);
    }
}

impl FlattenedPathSink for BoundsSink {
    fn begin_contour(&mut self, start: VectorPoint) {
        self.include(start);
    }

    fn segment(&mut self, _: VectorPoint, to: VectorPoint) {
        self.include(to);
    }

    fn end_contour(&mut self, _: bool) {}
}

struct IntersectionSink {
    sample_y: f32,
    intersections: [Intersection; MAX_INTERSECTIONS],
    len: usize,
    overflowed: bool,

    contour_start: Option<VectorPoint>,
    contour_end: Option<VectorPoint>,
}

impl IntersectionSink {
    fn new(sample_y: f32) -> Self {
        Self {
            sample_y,
            intersections: [Intersection::EMPTY; MAX_INTERSECTIONS],
            len: 0,
            overflowed: false,
            contour_start: None,
            contour_end: None,
        }
    }

    fn push(&mut self, intersection: Intersection) {
        if self.len >= MAX_INTERSECTIONS {
            self.overflowed = true;
            return;
        }

        self.intersections[self.len] = intersection;
        self.len += 1;
    }

    fn add_segment(&mut self, from: VectorPoint, to: VectorPoint) {
        let winding = if from.y <= self.sample_y && self.sample_y < to.y {
            1
        } else if to.y <= self.sample_y && self.sample_y < from.y {
            -1
        } else {
            return;
        };

        let height = to.y - from.y;

        if height == 0.0 {
            return;
        }

        let t = (self.sample_y - from.y) / height;
        let x = from.x + (to.x - from.x) * t;

        self.push(Intersection { x, winding });
    }

    fn sort(&mut self) {
        let mut index = 1;

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

    fn intersections(&self) -> &[Intersection] {
        &self.intersections[..self.len]
    }
}

impl FlattenedPathSink for IntersectionSink {
    fn begin_contour(&mut self, start: VectorPoint) {
        self.contour_start = Some(start);
        self.contour_end = Some(start);
    }

    fn segment(&mut self, from: VectorPoint, to: VectorPoint) {
        self.add_segment(from, to);
        self.contour_end = Some(to);
    }

    fn end_contour(&mut self, closed: bool) {
        if !closed
            && let (Some(end), Some(start)) = (self.contour_end, self.contour_start)
            && end != start
        {
            self.add_segment(end, start);
        }

        self.contour_start = None;
        self.contour_end = None;
    }
}

const AA_SAMPLES_PER_AXIS: usize = 4;
const AA_SAMPLE_COUNT: u8 = 16;
const AA_SAMPLE_OFFSETS: [f32; AA_SAMPLES_PER_AXIS] = [0.125, 0.375, 0.625, 0.875];

const MAX_SPANS: usize = MAX_INTERSECTIONS / 2;

#[derive(Clone, Copy)]
struct Span {
    left: f32,
    right: f32,
}

impl Span {
    const EMPTY: Self = Self {
        left: 0.0,
        right: 0.0,
    };
}

#[derive(Clone, Copy)]
struct SampleRow {
    spans: [Span; MAX_SPANS],
    len: usize,
}

impl SampleRow {
    const EMPTY: Self = Self {
        spans: [Span::EMPTY; MAX_SPANS],
        len: 0,
    };

    fn push(&mut self, left: f32, right: f32) -> bool {
        if right <= left {
            return true;
        }

        if self.len >= MAX_SPANS {
            return false;
        }

        self.spans[self.len] = Span { left, right };
        self.len += 1;

        true
    }

    fn contains_from(&self, x: f32, cursor: &mut usize) -> bool {
        while *cursor < self.len && x >= self.spans[*cursor].right {
            *cursor += 1;
        }

        if *cursor >= self.len {
            return false;
        }

        let span = self.spans[*cursor];

        x >= span.left && x < span.right
    }
}

pub(super) fn paint_filled_path<P>(
    painter: &mut P,
    path: VectorPath<'_>,
    transform: AffineTransform,
    fill: PathFill,
) where
    P: CanvasPainter + ?Sized,
{
    if path.is_empty() {
        return;
    }

    let mut bounds = BoundsSink::default();

    flatten_path(path, transform, &mut bounds);

    if !bounds.initialized {
        return;
    }

    let start_x = floor_to_i32(bounds.min_x);
    let end_x = ceil_to_i32(bounds.max_x);

    let start_y = floor_to_i32(bounds.min_y);
    let end_y = ceil_to_i32(bounds.max_y);

    let mut y = start_y;

    while y < end_y {
        let mut sample_rows = [SampleRow::EMPTY; AA_SAMPLES_PER_AXIS];
        let mut row_valid = true;

        for (sample_index, offset) in AA_SAMPLE_OFFSETS.iter().copied().enumerate() {
            let sample_y = y as f32 + offset;

            let Some(row) = build_sample_row(path, transform, fill.rule, sample_y) else {
                row_valid = false;
                break;
            };

            sample_rows[sample_index] = row;
        }

        if row_valid {
            rasterize_coverage_row(painter, &sample_rows, start_x, end_x, y, fill.color);
        }

        if y == i32::MAX {
            break;
        }

        y += 1;
    }
}

fn build_sample_row(
    path: VectorPath<'_>,
    transform: AffineTransform,
    fill_rule: FillRule,
    sample_y: f32,
) -> Option<SampleRow> {
    let mut intersections = IntersectionSink::new(sample_y);

    flatten_path(path, transform, &mut intersections);

    debug_assert!(
        !intersections.overflowed,
        "vector path exceeded bounded scanline intersection capacity"
    );

    if intersections.overflowed {
        return None;
    }

    intersections.sort();

    match fill_rule {
        FillRule::EvenOdd => build_even_odd_row(intersections.intersections()),
        FillRule::NonZero => build_non_zero_row(intersections.intersections()),
    }
}

fn build_even_odd_row(intersections: &[Intersection]) -> Option<SampleRow> {
    let mut row = SampleRow::EMPTY;
    let mut index = 0;

    while index + 1 < intersections.len() {
        if !row.push(intersections[index].x, intersections[index + 1].x) {
            debug_assert!(false, "vector path exceeded bounded span capacity");
            return None;
        }

        index += 2;
    }

    Some(row)
}

fn build_non_zero_row(intersections: &[Intersection]) -> Option<SampleRow> {
    let mut row = SampleRow::EMPTY;

    let mut winding = 0i16;
    let mut start = None;
    let mut index = 0;

    while index < intersections.len() {
        let x = intersections[index].x;
        let winding_before = winding;

        while index < intersections.len() && intersections[index].x == x {
            winding += i16::from(intersections[index].winding);
            index += 1;
        }

        if winding_before == 0 && winding != 0 {
            start = Some(x);
        } else if winding_before != 0
            && winding == 0
            && let Some(start_x) = start.take()
            && !row.push(start_x, x)
        {
            debug_assert!(false, "vector path exceeded bounded span capacity");
            return None;
        }
    }

    Some(row)
}

fn rasterize_coverage_row<P>(
    painter: &mut P,
    sample_rows: &[SampleRow; AA_SAMPLES_PER_AXIS],
    start_x: i32,
    end_x: i32,
    y: i32,
    color: Color,
) where
    P: CanvasPainter + ?Sized,
{
    let mut cursors = [0usize; AA_SAMPLES_PER_AXIS];
    let mut full_run_start = None;

    let mut x = start_x;

    while x < end_x {
        let coverage_samples = sample_coverage(sample_rows, &mut cursors, x);

        if coverage_samples == AA_SAMPLE_COUNT {
            if full_run_start.is_none() {
                full_run_start = Some(x);
            }
        } else {
            flush_full_run(painter, &mut full_run_start, x, y, color);

            if coverage_samples != 0 {
                painter.fill_pixel_coverage(
                    Point::new(px(x), px(y)),
                    color,
                    coverage_from_samples(coverage_samples),
                );
            }
        }

        if x == i32::MAX {
            break;
        }

        x += 1;
    }

    flush_full_run(painter, &mut full_run_start, end_x, y, color);
}

fn sample_coverage(
    sample_rows: &[SampleRow; AA_SAMPLES_PER_AXIS],
    cursors: &mut [usize; AA_SAMPLES_PER_AXIS],
    x: i32,
) -> u8 {
    let base_x = x as f32;
    let mut covered = 0u8;

    for (row, cursor) in sample_rows.iter().zip(cursors.iter_mut()) {
        for offset in AA_SAMPLE_OFFSETS {
            if row.contains_from(base_x + offset, cursor) {
                covered = covered.saturating_add(1);
            }
        }
    }

    covered
}

fn coverage_from_samples(samples: u8) -> u8 {
    debug_assert!(samples <= AA_SAMPLE_COUNT);

    if samples == 0 {
        return 0;
    }

    if samples == AA_SAMPLE_COUNT {
        return u8::MAX;
    }

    let numerator = u16::from(samples)
        .saturating_mul(u16::from(u8::MAX))
        .saturating_add(u16::from(AA_SAMPLE_COUNT / 2));

    u8::try_from(numerator / u16::from(AA_SAMPLE_COUNT)).unwrap_or(u8::MAX)
}

fn flush_full_run<P>(
    painter: &mut P,
    full_run_start: &mut Option<i32>,
    end_x: i32,
    y: i32,
    color: Color,
) where
    P: CanvasPainter + ?Sized,
{
    let Some(start_x) = full_run_start.take() else {
        return;
    };

    if end_x <= start_x {
        return;
    }

    painter.fill_rect(
        Rect::new(
            Point::new(px(start_x), px(y)),
            Size::new(px(end_x.saturating_sub(start_x)), px(1)),
        ),
        color,
    );
}

#[cfg(test)]
mod tests {
    use std::vec::Vec;

    use alloc::vec;

    use crate::{
        AffineTransform, CanvasPainter, Color, FillRule, PathCommand, PathFill, Pixels, Point,
        Rect, VectorPath, VectorPoint, px,
    };

    #[derive(Default)]
    struct RecordingPainter {
        fills: Vec<Rect>,
    }

    impl CanvasPainter for RecordingPainter {
        fn fill_rect(&mut self, rect: Rect, _: Color) {
            self.fills.push(rect);
        }

        fn stroke_rect(&mut self, _: Rect, _: Pixels, _: Color) {}
        fn line(&mut self, _: Point, _: Point, _: Pixels, _: Color) {}
        fn fill_circle(&mut self, _: Point, _: Pixels, _: Color) {}
        fn stroke_circle(&mut self, _: Point, _: Pixels, _: Pixels, _: Color) {}
    }

    fn painted_area(painter: &RecordingPainter) -> i32 {
        painter
            .fills
            .iter()
            .map(|rect| rect.size.width.get() * rect.size.height.get())
            .sum()
    }

    #[test]
    fn fills_closed_rectangle() {
        const COMMANDS: [PathCommand; 5] = [
            PathCommand::MoveTo(VectorPoint::new(0.0, 0.0)),
            PathCommand::LineTo(VectorPoint::new(4.0, 0.0)),
            PathCommand::LineTo(VectorPoint::new(4.0, 4.0)),
            PathCommand::LineTo(VectorPoint::new(0.0, 4.0)),
            PathCommand::Close,
        ];

        let mut painter = RecordingPainter::default();

        painter.fill_path(
            VectorPath::new(&COMMANDS),
            AffineTransform::IDENTITY,
            PathFill::new(Color::BLACK),
        );

        assert_eq!(painted_area(&painter), 16);
    }

    #[test]
    fn even_odd_fill_creates_hole() {
        const COMMANDS: [PathCommand; 10] = [
            PathCommand::MoveTo(VectorPoint::new(0.0, 0.0)),
            PathCommand::LineTo(VectorPoint::new(6.0, 0.0)),
            PathCommand::LineTo(VectorPoint::new(6.0, 6.0)),
            PathCommand::LineTo(VectorPoint::new(0.0, 6.0)),
            PathCommand::Close,
            PathCommand::MoveTo(VectorPoint::new(2.0, 2.0)),
            PathCommand::LineTo(VectorPoint::new(4.0, 2.0)),
            PathCommand::LineTo(VectorPoint::new(4.0, 4.0)),
            PathCommand::LineTo(VectorPoint::new(2.0, 4.0)),
            PathCommand::Close,
        ];

        let mut painter = RecordingPainter::default();

        painter.fill_path(
            VectorPath::new(&COMMANDS),
            AffineTransform::IDENTITY,
            PathFill::new(Color::BLACK).with_rule(FillRule::EvenOdd),
        );

        assert_eq!(painted_area(&painter), 32);
    }

    #[test]
    fn non_zero_fill_keeps_same_winding_inner_contour_filled() {
        const COMMANDS: [PathCommand; 10] = [
            PathCommand::MoveTo(VectorPoint::new(0.0, 0.0)),
            PathCommand::LineTo(VectorPoint::new(6.0, 0.0)),
            PathCommand::LineTo(VectorPoint::new(6.0, 6.0)),
            PathCommand::LineTo(VectorPoint::new(0.0, 6.0)),
            PathCommand::Close,
            PathCommand::MoveTo(VectorPoint::new(2.0, 2.0)),
            PathCommand::LineTo(VectorPoint::new(4.0, 2.0)),
            PathCommand::LineTo(VectorPoint::new(4.0, 4.0)),
            PathCommand::LineTo(VectorPoint::new(2.0, 4.0)),
            PathCommand::Close,
        ];

        let mut painter = RecordingPainter::default();

        painter.fill_path(
            VectorPath::new(&COMMANDS),
            AffineTransform::IDENTITY,
            PathFill::new(Color::BLACK),
        );

        assert_eq!(painted_area(&painter), 36);
    }

    #[derive(Default)]
    struct CoverageRecordingPainter {
        pixels: Vec<(Point, u8)>,
    }

    impl CanvasPainter for CoverageRecordingPainter {
        fn fill_rect(&mut self, _: Rect, _: Color) {}
        fn stroke_rect(&mut self, _: Rect, _: Pixels, _: Color) {}
        fn line(&mut self, _: Point, _: Point, _: Pixels, _: Color) {}
        fn fill_circle(&mut self, _: Point, _: Pixels, _: Color) {}
        fn stroke_circle(&mut self, _: Point, _: Pixels, _: Pixels, _: Color) {}

        fn fill_pixel_coverage(&mut self, point: Point, _: Color, coverage: u8) {
            self.pixels.push((point, coverage));
        }
    }

    #[test]
    fn antialiases_fractional_rectangle_edges() {
        const COMMANDS: [PathCommand; 5] = [
            PathCommand::MoveTo(VectorPoint::new(0.25, 0.0)),
            PathCommand::LineTo(VectorPoint::new(1.25, 0.0)),
            PathCommand::LineTo(VectorPoint::new(1.25, 1.0)),
            PathCommand::LineTo(VectorPoint::new(0.25, 1.0)),
            PathCommand::Close,
        ];

        let mut painter = CoverageRecordingPainter::default();

        painter.fill_path(
            VectorPath::new(&COMMANDS),
            AffineTransform::IDENTITY,
            PathFill::new(Color::BLACK),
        );

        assert_eq!(
            painter.pixels,
            vec![
                (Point::new(px(0), px(0)), 191),
                (Point::new(px(1), px(0)), 64),
            ]
        );
    }
}
