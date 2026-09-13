use crate::{
    AffineTransform, CanvasPainter, PathCommand, PathStroke, Point, VectorPath, VectorPoint, px,
};

const VECTOR_CURVE_PIXELS_PER_STEP: f32 = 2.0;
const VECTOR_MAX_CURVE_STEPS: usize = 32;

pub fn paint_stroked_path<P>(
    painter: &mut P,
    path: VectorPath<'_>,
    transform: AffineTransform,
    stroke: PathStroke,
) where
    P: CanvasPainter + ?Sized,
{
    if stroke.width.is_non_positive() {
        return;
    }

    let mut current = None;
    let mut contour_start = None;

    for command in path.commands() {
        match *command {
            PathCommand::MoveTo(point) => {
                let point = transform.map_point(point);
                current = Some(point);
                contour_start = Some(point);
            }
            PathCommand::LineTo(point) => {
                let Some(from) = current else {
                    continue;
                };

                let to = transform.map_point(point);

                paint_vector_segment(painter, from, to, stroke);

                current = Some(to);
            }
            PathCommand::QuadraticTo { control, to } => {
                let Some(from) = current else {
                    continue;
                };

                let control = transform.map_point(control);
                let to = transform.map_point(to);

                paint_quadratic_curve(painter, from, control, to, stroke);

                current = Some(to);
            }
            PathCommand::CubicTo {
                control_1,
                control_2,
                to,
            } => {
                let Some(from) = current else {
                    continue;
                };

                let control_1 = transform.map_point(control_1);
                let control_2 = transform.map_point(control_2);
                let to = transform.map_point(to);

                paint_cubic_curve(painter, from, control_1, control_2, to, stroke);

                current = Some(to);
            }
            PathCommand::Close => {
                let (Some(from), Some(to)) = (current, contour_start) else {
                    continue;
                };

                paint_vector_segment(painter, from, to, stroke);

                current = Some(to);
            }
        }
    }
}

fn paint_quadratic_curve<P>(
    painter: &mut P,
    start: VectorPoint,
    control: VectorPoint,
    end: VectorPoint,
    stroke: PathStroke,
) where
    P: CanvasPainter + ?Sized,
{
    let steps = vector_curve_steps(&[start, control, end]);

    let mut previous = start;

    for step in 1..=steps {
        let t = step as f32 / steps as f32;
        let inverse = 1.0 - t;

        let point = VectorPoint::new(
            inverse * inverse * start.x + 2.0 * inverse * t * control.x + t * t * end.x,
            inverse * inverse * start.y + 2.0 * inverse * t * control.y + t * t * end.y,
        );

        paint_vector_segment(painter, previous, point, stroke);

        previous = point;
    }
}

fn paint_cubic_curve<P>(
    painter: &mut P,
    start: VectorPoint,
    control_1: VectorPoint,
    control_2: VectorPoint,
    end: VectorPoint,
    stroke: PathStroke,
) where
    P: CanvasPainter + ?Sized,
{
    let steps = vector_curve_steps(&[start, control_1, control_2, end]);

    let mut previous = start;

    for step in 1..=steps {
        let t = step as f32 / steps as f32;
        let inverse = 1.0 - t;

        let inverse_2 = inverse * inverse;
        let t_2 = t * t;

        let point = VectorPoint::new(
            inverse_2 * inverse * start.x
                + 3.0 * inverse_2 * t * control_1.x
                + 3.0 * inverse * t_2 * control_2.x
                + t_2 * t * end.x,
            inverse_2 * inverse * start.y
                + 3.0 * inverse_2 * t * control_1.y
                + 3.0 * inverse * t_2 * control_2.y
                + t_2 * t * end.y,
        );

        paint_vector_segment(painter, previous, point, stroke);

        previous = point;
    }
}

fn paint_vector_segment<P>(
    painter: &mut P,
    start: VectorPoint,
    end: VectorPoint,
    stroke: PathStroke,
) where
    P: CanvasPainter + ?Sized,
{
    let start = vector_point_to_pixel_point(start);
    let end = vector_point_to_pixel_point(end);

    if start == end {
        return;
    }

    painter.line(start, end, stroke.width, stroke.color);
}

fn vector_curve_steps(points: &[VectorPoint]) -> usize {
    if points.len() < 2 {
        return 1;
    }

    let mut length = 0.0;

    for pair in points.windows(2) {
        length += vector_approximate_distance(pair[0], pair[1]);
    }

    let estimate = length / VECTOR_CURVE_PIXELS_PER_STEP;
    let truncated = estimate as usize;

    let mut steps = if (truncated as f32) < estimate {
        truncated.saturating_add(1)
    } else {
        truncated
    };

    if steps == 0 {
        steps = 1;
    }

    steps.min(VECTOR_MAX_CURVE_STEPS)
}

fn vector_approximate_distance(from: VectorPoint, to: VectorPoint) -> f32 {
    vector_abs(to.x - from.x) + vector_abs(to.y - from.y)
}

fn vector_abs(value: f32) -> f32 {
    if value < 0.0 { -value } else { value }
}

fn vector_point_to_pixel_point(point: VectorPoint) -> Point {
    Point::new(
        px(round_vector_coordinate(point.x)),
        px(round_vector_coordinate(point.y)),
    )
}

fn round_vector_coordinate(value: f32) -> i32 {
    if !value.is_finite() {
        return 0;
    }

    if value >= 0.0 {
        (value + 0.5) as i32
    } else {
        (value - 0.5) as i32
    }
}

#[cfg(test)]
mod tests {
    use std::vec::Vec;

    use crate::{
        AffineTransform, CanvasPainter, Color, PathCommand, PathStroke, Pixels, Point, Rect,
        VectorPath, VectorPoint, px,
    };

    #[derive(Default)]
    struct RecordingPainter {
        lines: Vec<(Point, Point, Pixels, Color)>,
    }

    impl CanvasPainter for RecordingPainter {
        fn fill_rect(&mut self, _: Rect, _: Color) {}
        fn stroke_rect(&mut self, _: Rect, _: Pixels, _: Color) {}
        fn fill_circle(&mut self, _: Point, _: Pixels, _: Color) {}
        fn stroke_circle(&mut self, _: Point, _: Pixels, _: Pixels, _: Color) {}

        fn line(&mut self, start: Point, end: Point, width: Pixels, color: Color) {
            self.lines.push((start, end, width, color));
        }
    }

    #[test]
    fn stroke_path_draws_lines_and_closes_contour() {
        const COMMANDS: [PathCommand; 4] = [
            PathCommand::MoveTo(VectorPoint::new(0.0, 0.0)),
            PathCommand::LineTo(VectorPoint::new(4.0, 0.0)),
            PathCommand::LineTo(VectorPoint::new(4.0, 2.0)),
            PathCommand::Close,
        ];

        let mut painter = RecordingPainter::default();

        painter.stroke_path(
            VectorPath::new(&COMMANDS),
            AffineTransform::scale_translate(2.0, 2.0, 1.0, 3.0),
            PathStroke::new(px(2), Color::BLACK),
        );

        assert_eq!(painter.lines.len(), 3);
        assert_eq!(
            painter.lines[0],
            (
                Point::new(px(1), px(3)),
                Point::new(px(9), px(3)),
                px(2),
                Color::BLACK,
            ),
        );
        assert_eq!(
            painter.lines[1],
            (
                Point::new(px(9), px(3)),
                Point::new(px(9), px(7)),
                px(2),
                Color::BLACK,
            ),
        );
        assert_eq!(
            painter.lines[2],
            (
                Point::new(px(9), px(7)),
                Point::new(px(1), px(3)),
                px(2),
                Color::BLACK,
            ),
        );
    }

    #[test]
    fn stroke_path_flattens_quadratic_curve() {
        const COMMANDS: [PathCommand; 2] = [
            PathCommand::MoveTo(VectorPoint::new(0.0, 0.0)),
            PathCommand::QuadraticTo {
                control: VectorPoint::new(5.0, 10.0),
                to: VectorPoint::new(10.0, 0.0),
            },
        ];

        let mut painter = RecordingPainter::default();

        painter.stroke_path(
            VectorPath::new(&COMMANDS),
            AffineTransform::IDENTITY,
            PathStroke::new(px(1), Color::BLACK),
        );

        assert!(painter.lines.len() > 1);
        assert_eq!(
            painter.lines.first().map(|line| line.0),
            Some(Point::new(px(0), px(0))),
        );
        assert_eq!(
            painter.lines.last().map(|line| line.1),
            Some(Point::new(px(10), px(0))),
        );
    }

    #[test]
    fn stroke_path_flattens_cubic_curve() {
        const COMMANDS: [PathCommand; 2] = [
            PathCommand::MoveTo(VectorPoint::new(0.0, 0.0)),
            PathCommand::CubicTo {
                control_1: VectorPoint::new(0.0, 10.0),
                control_2: VectorPoint::new(10.0, 10.0),
                to: VectorPoint::new(10.0, 0.0),
            },
        ];

        let mut painter = RecordingPainter::default();

        painter.stroke_path(
            VectorPath::new(&COMMANDS),
            AffineTransform::IDENTITY,
            PathStroke::new(px(1), Color::BLACK),
        );

        assert!(painter.lines.len() > 1);

        assert_eq!(
            painter.lines.first().map(|line| line.0),
            Some(Point::new(px(0), px(0))),
        );
        assert_eq!(
            painter.lines.last().map(|line| line.1),
            Some(Point::new(px(10), px(0))),
        );
    }

    #[test]
    fn stroke_path_skips_non_positive_stroke_width() {
        const COMMANDS: [PathCommand; 2] = [
            PathCommand::MoveTo(VectorPoint::new(0.0, 0.0)),
            PathCommand::LineTo(VectorPoint::new(10.0, 10.0)),
        ];

        let mut painter = RecordingPainter::default();

        painter.stroke_path(
            VectorPath::new(&COMMANDS),
            AffineTransform::IDENTITY,
            PathStroke::new(px(0), Color::BLACK),
        );

        assert!(painter.lines.is_empty());
    }
}
