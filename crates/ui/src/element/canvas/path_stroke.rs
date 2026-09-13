use crate::{
    AffineTransform, CanvasPainter, Color, PathCommand, PathFill, PathStroke, Point, StrokeCap,
    StrokeJoin, VectorPath, VectorPoint, px,
};

use super::{
    path::{FlattenedPathSink, flatten_path, round_to_i32},
    path_fill::paint_filled_path,
};

const PARALLEL_EPSILON: f32 = 0.0001;

#[derive(Clone, Copy)]
struct Segment {
    from: VectorPoint,
    to: VectorPoint,
}

struct StrokeSink<'a, P: ?Sized> {
    painter: &'a mut P,
    stroke: PathStroke,

    first: Option<Segment>,
    previous: Option<Segment>,
}

impl<'a, P> StrokeSink<'a, P>
where
    P: CanvasPainter + ?Sized,
{
    fn new(painter: &'a mut P, stroke: PathStroke) -> Self {
        Self {
            painter,
            stroke,
            first: None,
            previous: None,
        }
    }

    fn reset(&mut self) {
        self.first = None;
        self.previous = None;
    }
}

impl<P> FlattenedPathSink for StrokeSink<'_, P>
where
    P: CanvasPainter + ?Sized,
{
    fn begin_contour(&mut self, _: VectorPoint) {
        self.reset();
    }

    fn segment(&mut self, from: VectorPoint, to: VectorPoint) {
        if from == to {
            return;
        }

        let segment = Segment { from, to };

        self.painter.line(
            pixel_point(from),
            pixel_point(to),
            self.stroke.width,
            self.stroke.color,
        );

        if let Some(previous) = self.previous {
            paint_join(self.painter, previous, segment, self.stroke);
        } else {
            self.first = Some(segment);
        }

        self.previous = Some(segment);
    }

    fn end_contour(&mut self, closed: bool) {
        let (Some(first), Some(last)) = (self.first, self.previous) else {
            self.reset();
            return;
        };

        if closed {
            paint_join(self.painter, last, first, self.stroke);
        } else {
            paint_cap(self.painter, first, true, self.stroke);

            paint_cap(self.painter, last, false, self.stroke);
        }

        self.reset();
    }
}

pub(super) fn paint_stroked_path<P>(
    painter: &mut P,
    path: VectorPath<'_>,
    transform: AffineTransform,
    stroke: PathStroke,
) where
    P: CanvasPainter + ?Sized,
{
    if path.is_empty() || stroke.width.is_non_positive() {
        return;
    }

    let mut sink = StrokeSink::new(painter, stroke);

    flatten_path(path, transform, &mut sink);
}

fn paint_cap<P>(painter: &mut P, segment: Segment, start: bool, stroke: PathStroke)
where
    P: CanvasPainter + ?Sized,
{
    match stroke.cap {
        StrokeCap::Butt => {}

        StrokeCap::Round => {
            let Some(radius) = round_radius(stroke) else {
                return;
            };

            let center = if start { segment.from } else { segment.to };

            painter.fill_circle(pixel_point(center), radius, stroke.color);
        }

        StrokeCap::Square => {
            let Some(direction) = normalized_direction(segment) else {
                return;
            };

            let radius = stroke.width.get() as f32 / 2.0;

            let endpoint = if start { segment.from } else { segment.to };

            let multiplier = if start { -radius } else { radius };

            let extended = VectorPoint::new(
                endpoint.x + direction.x * multiplier,
                endpoint.y + direction.y * multiplier,
            );

            painter.line(
                pixel_point(endpoint),
                pixel_point(extended),
                stroke.width,
                stroke.color,
            );
        }
    }
}

fn paint_join<P>(painter: &mut P, previous: Segment, next: Segment, stroke: PathStroke)
where
    P: CanvasPainter + ?Sized,
{
    let Some(previous_direction) = normalized_direction(previous) else {
        return;
    };

    let Some(next_direction) = normalized_direction(next) else {
        return;
    };

    let cross_value = cross(previous_direction, next_direction);

    if cross_value.abs() < PARALLEL_EPSILON {
        return;
    }

    let vertex = previous.to;

    match stroke.join {
        StrokeJoin::Round => {
            let Some(radius) = round_radius(stroke) else {
                return;
            };

            painter.fill_circle(pixel_point(vertex), radius, stroke.color);
        }

        StrokeJoin::Bevel => {
            let radius = stroke.width.get() as f32 / 2.0;

            let (outer_previous, outer_next) = outer_join_points(
                vertex,
                previous_direction,
                next_direction,
                cross_value,
                radius,
            );

            fill_triangle(painter, vertex, outer_previous, outer_next, stroke.color);
        }

        StrokeJoin::Miter => {
            paint_miter_join(
                painter,
                vertex,
                previous_direction,
                next_direction,
                cross_value,
                stroke,
            );
        }
    }
}

fn paint_miter_join<P>(
    painter: &mut P,
    vertex: VectorPoint,
    previous_direction: VectorPoint,
    next_direction: VectorPoint,
    cross_value: f32,
    stroke: PathStroke,
) where
    P: CanvasPainter + ?Sized,
{
    let radius = stroke.width.get() as f32 / 2.0;

    if radius <= 0.0 {
        return;
    }

    let (outer_previous, outer_next) = outer_join_points(
        vertex,
        previous_direction,
        next_direction,
        cross_value,
        radius,
    );

    let denominator = cross(previous_direction, next_direction);

    if denominator.abs() < PARALLEL_EPSILON {
        return;
    }

    let delta = subtract(outer_next, outer_previous);

    let t = cross(delta, next_direction) / denominator;

    let miter = VectorPoint::new(
        outer_previous.x + previous_direction.x * t,
        outer_previous.y + previous_direction.y * t,
    );

    let miter_vector = subtract(miter, vertex);

    let miter_length = vector_length(miter_vector);

    let limit = stroke.miter_limit;

    if !miter_length.is_finite() || limit <= 0.0 || miter_length / radius > limit {
        fill_triangle(painter, vertex, outer_previous, outer_next, stroke.color);

        return;
    }

    fill_triangle(painter, vertex, outer_previous, miter, stroke.color);

    fill_triangle(painter, vertex, miter, outer_next, stroke.color);
}

fn outer_join_points(
    vertex: VectorPoint,
    previous_direction: VectorPoint,
    next_direction: VectorPoint,
    cross_value: f32,
    radius: f32,
) -> (VectorPoint, VectorPoint) {
    let previous_normal = outer_normal(previous_direction, cross_value);

    let next_normal = outer_normal(next_direction, cross_value);

    (
        VectorPoint::new(
            vertex.x + previous_normal.x * radius,
            vertex.y + previous_normal.y * radius,
        ),
        VectorPoint::new(
            vertex.x + next_normal.x * radius,
            vertex.y + next_normal.y * radius,
        ),
    )
}

fn outer_normal(direction: VectorPoint, cross_value: f32) -> VectorPoint {
    if cross_value > 0.0 {
        VectorPoint::new(direction.y, -direction.x)
    } else {
        VectorPoint::new(-direction.y, direction.x)
    }
}

fn normalized_direction(segment: Segment) -> Option<VectorPoint> {
    let direction = subtract(segment.to, segment.from);

    let length = vector_length(direction);

    if length <= 0.0 || !length.is_finite() {
        return None;
    }

    Some(VectorPoint::new(direction.x / length, direction.y / length))
}

fn vector_length(vector: VectorPoint) -> f32 {
    sqrt(vector.x * vector.x + vector.y * vector.y)
}

fn sqrt(value: f32) -> f32 {
    if value <= 0.0 || !value.is_finite() {
        return 0.0;
    }

    let mut estimate = if value >= 1.0 { value } else { 1.0 };

    let mut iteration = 0;

    while iteration < 8 {
        estimate = 0.5 * (estimate + value / estimate);

        iteration += 1;
    }

    estimate
}

fn cross(a: VectorPoint, b: VectorPoint) -> f32 {
    a.x * b.y - a.y * b.x
}

fn subtract(a: VectorPoint, b: VectorPoint) -> VectorPoint {
    VectorPoint::new(a.x - b.x, a.y - b.y)
}

fn round_radius(stroke: PathStroke) -> Option<crate::Pixels> {
    let width = stroke.width.get();

    if width <= 1 {
        return None;
    }

    Some(px(width.saturating_add(1) / 2))
}

fn pixel_point(point: VectorPoint) -> Point {
    Point::new(px(round_to_i32(point.x)), px(round_to_i32(point.y)))
}

fn fill_triangle<P>(painter: &mut P, a: VectorPoint, b: VectorPoint, c: VectorPoint, color: Color)
where
    P: CanvasPainter + ?Sized,
{
    let commands = [
        PathCommand::MoveTo(a),
        PathCommand::LineTo(b),
        PathCommand::LineTo(c),
        PathCommand::Close,
    ];

    paint_filled_path(
        painter,
        VectorPath::new(&commands),
        AffineTransform::IDENTITY,
        PathFill::new(color),
    );
}

#[cfg(test)]
mod tests {
    use std::vec::Vec;

    use crate::{
        AffineTransform, CanvasPainter, Color, PathCommand, PathStroke, Pixels, Point, Rect,
        StrokeCap, StrokeJoin, VectorPath, VectorPoint, px,
    };

    #[derive(Default)]
    struct RecordingPainter {
        lines: Vec<(Point, Point, Pixels, Color)>,
        circles: Vec<(Point, Pixels, Color)>,
        fills: Vec<Rect>,
    }

    impl CanvasPainter for RecordingPainter {
        fn fill_rect(&mut self, rect: Rect, _: Color) {
            self.fills.push(rect);
        }

        fn stroke_rect(&mut self, _: Rect, _: Pixels, _: Color) {}

        fn line(&mut self, start: Point, end: Point, width: Pixels, color: Color) {
            self.lines.push((start, end, width, color));
        }

        fn fill_circle(&mut self, center: Point, radius: Pixels, color: Color) {
            self.circles.push((center, radius, color));
        }

        fn stroke_circle(&mut self, _: Point, _: Pixels, _: Pixels, _: Color) {}
    }

    #[test]
    fn round_caps_are_added_only_to_open_contour_ends() {
        const COMMANDS: [PathCommand; 2] = [
            PathCommand::MoveTo(VectorPoint::new(0.0, 0.0)),
            PathCommand::LineTo(VectorPoint::new(10.0, 0.0)),
        ];

        let mut painter = RecordingPainter::default();

        painter.stroke_path(
            VectorPath::new(&COMMANDS),
            AffineTransform::IDENTITY,
            PathStroke::new(px(4), Color::BLACK).with_cap(StrokeCap::Round),
        );

        assert_eq!(painter.circles.len(), 2);
        assert_eq!(painter.circles[0].0, Point::new(px(0), px(0)));
        assert_eq!(painter.circles[1].0, Point::new(px(10), px(0)));
    }

    #[test]
    fn square_caps_extend_open_contour() {
        const COMMANDS: [PathCommand; 2] = [
            PathCommand::MoveTo(VectorPoint::new(2.0, 4.0)),
            PathCommand::LineTo(VectorPoint::new(10.0, 4.0)),
        ];

        let mut painter = RecordingPainter::default();

        painter.stroke_path(
            VectorPath::new(&COMMANDS),
            AffineTransform::IDENTITY,
            PathStroke::new(px(4), Color::BLACK).with_cap(StrokeCap::Square),
        );

        assert_eq!(painter.lines.len(), 3);

        assert_eq!(painter.lines[1].1, Point::new(px(0), px(4)),);

        assert_eq!(painter.lines[2].1, Point::new(px(12), px(4)),);
    }

    #[test]
    fn round_join_paints_shared_vertex() {
        const COMMANDS: [PathCommand; 3] = [
            PathCommand::MoveTo(VectorPoint::new(0.0, 0.0)),
            PathCommand::LineTo(VectorPoint::new(10.0, 0.0)),
            PathCommand::LineTo(VectorPoint::new(10.0, 10.0)),
        ];

        let mut painter = RecordingPainter::default();

        painter.stroke_path(
            VectorPath::new(&COMMANDS),
            AffineTransform::IDENTITY,
            PathStroke::new(px(4), Color::BLACK).with_join(StrokeJoin::Round),
        );

        assert_eq!(painter.circles.len(), 1);
        assert_eq!(painter.circles[0].0, Point::new(px(10), px(0)),);
    }

    #[test]
    fn bevel_join_fills_corner() {
        const COMMANDS: [PathCommand; 3] = [
            PathCommand::MoveTo(VectorPoint::new(0.0, 0.0)),
            PathCommand::LineTo(VectorPoint::new(10.0, 0.0)),
            PathCommand::LineTo(VectorPoint::new(10.0, 10.0)),
        ];

        let mut painter = RecordingPainter::default();

        painter.stroke_path(
            VectorPath::new(&COMMANDS),
            AffineTransform::IDENTITY,
            PathStroke::new(px(4), Color::BLACK).with_join(StrokeJoin::Bevel),
        );

        assert!(!painter.fills.is_empty());
    }

    #[test]
    fn miter_join_fills_corner() {
        const COMMANDS: [PathCommand; 3] = [
            PathCommand::MoveTo(VectorPoint::new(0.0, 0.0)),
            PathCommand::LineTo(VectorPoint::new(10.0, 0.0)),
            PathCommand::LineTo(VectorPoint::new(10.0, 10.0)),
        ];

        let mut painter = RecordingPainter::default();

        painter.stroke_path(
            VectorPath::new(&COMMANDS),
            AffineTransform::IDENTITY,
            PathStroke::new(px(4), Color::BLACK).with_join(StrokeJoin::Miter),
        );

        assert!(!painter.fills.is_empty());
    }
}
