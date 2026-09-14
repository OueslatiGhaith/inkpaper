use crate::{
    AffineTransform, CanvasPainter, Color, PathCommand, PathFill, PathStroke, StrokeCap,
    StrokeJoin, VectorPath, VectorPoint,
};

use super::{
    path::{FlattenedPathSink, flatten_path},
    path_fill::paint_filled_path,
};

const PARALLEL_EPSILON: f32 = 0.0001;
const CIRCLE_KAPPA: f32 = 0.552_284_8;

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

        paint_segment(self.painter, segment, self.stroke);

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
    if path.is_empty() || stroke_radius(stroke).is_none() {
        return;
    }

    let mut sink = StrokeSink::new(painter, stroke);

    flatten_path(path, transform, &mut sink);
}

fn stroke_radius(stroke: PathStroke) -> Option<f32> {
    if !stroke.width.is_finite() || stroke.width <= 0.0 {
        return None;
    }

    Some(stroke.width / 2.0)
}

fn paint_segment<P>(painter: &mut P, segment: Segment, stroke: PathStroke)
where
    P: CanvasPainter + ?Sized,
{
    let Some(direction) = normalized_direction(segment) else {
        return;
    };

    let Some(radius) = stroke_radius(stroke) else {
        return;
    };

    let normal = VectorPoint::new(-direction.y * radius, direction.x * radius);

    let start_left = VectorPoint::new(segment.from.x + normal.x, segment.from.y + normal.y);

    let end_left = VectorPoint::new(segment.to.x + normal.x, segment.to.y + normal.y);

    let end_right = VectorPoint::new(segment.to.x - normal.x, segment.to.y - normal.y);

    let start_right = VectorPoint::new(segment.from.x - normal.x, segment.from.y - normal.y);

    fill_quad(
        painter,
        start_left,
        end_left,
        end_right,
        start_right,
        stroke.color,
    );
}

fn fill_quad<P>(
    painter: &mut P,
    a: VectorPoint,
    b: VectorPoint,
    c: VectorPoint,
    d: VectorPoint,
    color: Color,
) where
    P: CanvasPainter + ?Sized,
{
    let commands = [
        PathCommand::MoveTo(a),
        PathCommand::LineTo(b),
        PathCommand::LineTo(c),
        PathCommand::LineTo(d),
        PathCommand::Close,
    ];

    paint_filled_path(
        painter,
        VectorPath::new(&commands),
        AffineTransform::IDENTITY,
        PathFill::new(color),
    );
}

fn fill_disk<P>(painter: &mut P, center: VectorPoint, radius: f32, color: Color)
where
    P: CanvasPainter + ?Sized,
{
    if radius <= 0.0 || !radius.is_finite() {
        return;
    }

    let control = radius * CIRCLE_KAPPA;

    let commands = [
        PathCommand::MoveTo(VectorPoint::new(center.x + radius, center.y)),
        PathCommand::CubicTo {
            control_1: VectorPoint::new(center.x + radius, center.y + control),
            control_2: VectorPoint::new(center.x + control, center.y + radius),
            to: VectorPoint::new(center.x, center.y + radius),
        },
        PathCommand::CubicTo {
            control_1: VectorPoint::new(center.x - control, center.y + radius),
            control_2: VectorPoint::new(center.x - radius, center.y + control),
            to: VectorPoint::new(center.x - radius, center.y),
        },
        PathCommand::CubicTo {
            control_1: VectorPoint::new(center.x - radius, center.y - control),
            control_2: VectorPoint::new(center.x - control, center.y - radius),
            to: VectorPoint::new(center.x, center.y - radius),
        },
        PathCommand::CubicTo {
            control_1: VectorPoint::new(center.x + control, center.y - radius),
            control_2: VectorPoint::new(center.x + radius, center.y - control),
            to: VectorPoint::new(center.x + radius, center.y),
        },
        PathCommand::Close,
    ];

    paint_filled_path(
        painter,
        VectorPath::new(&commands),
        AffineTransform::IDENTITY,
        PathFill::new(color),
    );
}

fn paint_cap<P>(painter: &mut P, segment: Segment, start: bool, stroke: PathStroke)
where
    P: CanvasPainter + ?Sized,
{
    let Some(radius) = stroke_radius(stroke) else {
        return;
    };

    match stroke.cap {
        StrokeCap::Butt => {}

        StrokeCap::Round => {
            let center = if start { segment.from } else { segment.to };

            fill_disk(painter, center, radius, stroke.color);
        }

        StrokeCap::Square => {
            let Some(direction) = normalized_direction(segment) else {
                return;
            };

            let endpoint = if start { segment.from } else { segment.to };

            let multiplier = if start { -radius } else { radius };

            let extended = VectorPoint::new(
                endpoint.x + direction.x * multiplier,
                endpoint.y + direction.y * multiplier,
            );

            let cap = if start {
                Segment {
                    from: extended,
                    to: endpoint,
                }
            } else {
                Segment {
                    from: endpoint,
                    to: extended,
                }
            };

            paint_segment(painter, cap, stroke);
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

    let Some(radius) = stroke_radius(stroke) else {
        return;
    };

    let vertex = previous.to;

    match stroke.join {
        StrokeJoin::Round => {
            fill_disk(painter, vertex, radius, stroke.color);
        }

        StrokeJoin::Bevel => {
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
    let Some(radius) = stroke_radius(stroke) else {
        return;
    };

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
        fills: Vec<Rect>,
        coverage_pixels: Vec<(Point, u8)>,
    }

    impl CanvasPainter for RecordingPainter {
        fn fill_rect(&mut self, rect: Rect, _: Color) {
            self.fills.push(rect);
        }

        fn stroke_rect(&mut self, _: Rect, _: Pixels, _: Color) {}
        fn line(&mut self, _: Point, _: Point, _: Pixels, _: Color) {}
        fn fill_circle(&mut self, _: Point, _: Pixels, _: Color) {}
        fn stroke_circle(&mut self, _: Point, _: Pixels, _: Pixels, _: Color) {}

        fn fill_pixel_coverage(&mut self, point: Point, _: Color, coverage: u8) {
            if coverage != 0 {
                self.coverage_pixels.push((point, coverage));
            }
        }
    }

    fn painted_bounds(painter: &RecordingPainter) -> Option<(i32, i32, i32, i32)> {
        let mut bounds: Option<(i32, i32, i32, i32)> = None;

        for rect in &painter.fills {
            extend_bounds(
                &mut bounds,
                rect.origin.x.get(),
                rect.origin.y.get(),
                rect.origin.x.get().saturating_add(rect.size.width.get()),
                rect.origin.y.get().saturating_add(rect.size.height.get()),
            );
        }

        for &(point, coverage) in &painter.coverage_pixels {
            if coverage == 0 {
                continue;
            }

            extend_bounds(
                &mut bounds,
                point.x.get(),
                point.y.get(),
                point.x.get().saturating_add(1),
                point.y.get().saturating_add(1),
            );
        }

        bounds
    }

    fn extend_bounds(
        bounds: &mut Option<(i32, i32, i32, i32)>,
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    ) {
        match bounds {
            Some((min_x, min_y, max_x, max_y)) => {
                *min_x = (*min_x).min(left);
                *min_y = (*min_y).min(top);
                *max_x = (*max_x).max(right);
                *max_y = (*max_y).max(bottom);
            }
            None => {
                *bounds = Some((left, top, right, bottom));
            }
        }
    }

    fn coverage_at(painter: &RecordingPainter, point: Point) -> u8 {
        if painter.fills.iter().any(|rect| rect.contains(point)) {
            return u8::MAX;
        }

        painter
            .coverage_pixels
            .iter()
            .rev()
            .find_map(|&(painted_point, coverage)| (painted_point == point).then_some(coverage))
            .unwrap_or(0)
    }

    #[test]
    fn stroke_preserves_subpixel_segment_position() {
        const COMMANDS: [PathCommand; 2] = [
            PathCommand::MoveTo(VectorPoint::new(2.666_666_7, 1.0)),
            PathCommand::LineTo(VectorPoint::new(2.666_666_7, 9.0)),
        ];

        let mut painter = RecordingPainter::default();

        painter.stroke_path(
            VectorPath::new(&COMMANDS),
            AffineTransform::IDENTITY,
            PathStroke::new(3.0, Color::BLACK),
        );

        assert_eq!(coverage_at(&painter, Point::new(px(0), px(4))), 0);
        assert_eq!(coverage_at(&painter, Point::new(px(1), px(4))), 191);
        assert_eq!(coverage_at(&painter, Point::new(px(2), px(4))), 255);
        assert_eq!(coverage_at(&painter, Point::new(px(3), px(4))), 255);
        assert_eq!(coverage_at(&painter, Point::new(px(4), px(4))), 64);
        assert_eq!(coverage_at(&painter, Point::new(px(5), px(4))), 0);
    }

    #[test]
    fn round_caps_extend_open_contour() {
        const COMMANDS: [PathCommand; 2] = [
            PathCommand::MoveTo(VectorPoint::new(4.0, 4.0)),
            PathCommand::LineTo(VectorPoint::new(8.0, 4.0)),
        ];

        let mut painter = RecordingPainter::default();

        painter.stroke_path(
            VectorPath::new(&COMMANDS),
            AffineTransform::IDENTITY,
            PathStroke::new(4.0, Color::BLACK).with_cap(StrokeCap::Round),
        );

        assert_eq!(painted_bounds(&painter), Some((2, 2, 10, 6)));
    }

    #[test]
    fn square_caps_extend_open_contour() {
        const COMMANDS: [PathCommand; 2] = [
            PathCommand::MoveTo(VectorPoint::new(4.0, 4.0)),
            PathCommand::LineTo(VectorPoint::new(8.0, 4.0)),
        ];

        let mut painter = RecordingPainter::default();

        painter.stroke_path(
            VectorPath::new(&COMMANDS),
            AffineTransform::IDENTITY,
            PathStroke::new(4.0, Color::BLACK).with_cap(StrokeCap::Square),
        );

        assert_eq!(painted_bounds(&painter), Some((2, 2, 10, 6)));
    }

    #[test]
    fn bevel_join_rasterizes_with_filled_geometry() {
        const COMMANDS: [PathCommand; 3] = [
            PathCommand::MoveTo(VectorPoint::new(2.0, 2.0)),
            PathCommand::LineTo(VectorPoint::new(8.0, 2.0)),
            PathCommand::LineTo(VectorPoint::new(8.0, 8.0)),
        ];

        let mut painter = RecordingPainter::default();

        painter.stroke_path(
            VectorPath::new(&COMMANDS),
            AffineTransform::IDENTITY,
            PathStroke::new(4.0, Color::BLACK).with_join(StrokeJoin::Bevel),
        );

        assert!(!painter.fills.is_empty());
    }

    #[test]
    fn miter_join_rasterizes_with_filled_geometry() {
        const COMMANDS: [PathCommand; 3] = [
            PathCommand::MoveTo(VectorPoint::new(2.0, 2.0)),
            PathCommand::LineTo(VectorPoint::new(8.0, 2.0)),
            PathCommand::LineTo(VectorPoint::new(8.0, 8.0)),
        ];

        let mut painter = RecordingPainter::default();

        painter.stroke_path(
            VectorPath::new(&COMMANDS),
            AffineTransform::IDENTITY,
            PathStroke::new(4.0, Color::BLACK).with_join(StrokeJoin::Miter),
        );

        assert!(!painter.fills.is_empty());
    }

    #[test]
    fn fractional_stroke_width_is_not_rounded_before_rasterization() {
        const COMMANDS: [PathCommand; 2] = [
            PathCommand::MoveTo(VectorPoint::new(2.0, 1.0)),
            PathCommand::LineTo(VectorPoint::new(2.0, 9.0)),
        ];

        let mut painter = RecordingPainter::default();

        painter.stroke_path(
            VectorPath::new(&COMMANDS),
            AffineTransform::IDENTITY,
            PathStroke::new(8.0 / 3.0, Color::BLACK),
        );

        assert_eq!(coverage_at(&painter, Point::new(px(-1), px(4))), 0);
        assert_eq!(coverage_at(&painter, Point::new(px(0), px(4))), 64);
        assert_eq!(coverage_at(&painter, Point::new(px(1), px(4))), 255);
        assert_eq!(coverage_at(&painter, Point::new(px(2), px(4))), 255);
        assert_eq!(coverage_at(&painter, Point::new(px(3), px(4))), 64);
        assert_eq!(coverage_at(&painter, Point::new(px(4), px(4))), 0);
    }
}
