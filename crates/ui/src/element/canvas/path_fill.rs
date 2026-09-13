use crate::{
    AffineTransform, CanvasPainter, FillRule, PathFill, Point, Rect, Size, VectorPath, VectorPoint,
    px,
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

    let mut y = ceil_to_i32(bounds.min_y - 0.5);
    let end_y = ceil_to_i32(bounds.max_y - 0.5);

    while y < end_y {
        let sample_y = y as f32 + 0.5;

        let mut intersections = IntersectionSink::new(sample_y);

        flatten_path(path, transform, &mut intersections);

        debug_assert!(
            !intersections.overflowed,
            "vector path exceeded bounded scanline intersection capacity"
        );

        if !intersections.overflowed {
            intersections.sort();

            match fill.rule {
                FillRule::EvenOdd => fill_even_odd(painter, y, intersections.intersections(), fill),
                FillRule::NonZero => fill_non_zero(painter, y, intersections.intersections(), fill),
            }
        }

        if y == i32::MAX {
            break;
        }

        y += 1;
    }
}

fn fill_even_odd<P>(painter: &mut P, y: i32, intersections: &[Intersection], fill: PathFill)
where
    P: CanvasPainter + ?Sized,
{
    let mut index = 0;

    while index + 1 < intersections.len() {
        fill_span(
            painter,
            y,
            intersections[index].x,
            intersections[index + 1].x,
            fill,
        );

        index += 2;
    }
}

fn fill_non_zero<P>(painter: &mut P, y: i32, intersections: &[Intersection], fill: PathFill)
where
    P: CanvasPainter + ?Sized,
{
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
        {
            fill_span(painter, y, start_x, x, fill);
        }
    }
}

fn fill_span<P>(painter: &mut P, y: i32, left: f32, right: f32, fill: PathFill)
where
    P: CanvasPainter + ?Sized,
{
    if right <= left {
        return;
    }

    let start = ceil_to_i32(left - 0.5);
    let end = ceil_to_i32(right - 0.5);

    if end <= start {
        return;
    }

    painter.fill_rect(
        Rect::new(
            Point::new(px(start), px(y)),
            Size::new(px(end.saturating_sub(start)), px(1)),
        ),
        fill.color,
    );
}

#[cfg(test)]
mod tests {
    use std::vec::Vec;

    use crate::{
        AffineTransform, CanvasPainter, Color, FillRule, PathCommand, PathFill, Pixels, Point,
        Rect, VectorPath, VectorPoint,
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
}
