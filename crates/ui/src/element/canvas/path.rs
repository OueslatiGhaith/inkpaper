use crate::{AffineTransform, PathCommand, VectorPath, VectorPoint};

const CURVE_PIXELS_PER_STEP: f32 = 2.0;
const MAX_CURVE_STEPS: usize = 32;

pub(super) trait FlattenedPathSink {
    fn begin_contour(&mut self, start: VectorPoint);
    fn segment(&mut self, from: VectorPoint, to: VectorPoint);
    fn end_contour(&mut self, closed: bool);
}

pub(super) fn flatten_path<S>(path: VectorPath<'_>, transform: AffineTransform, sink: &mut S)
where
    S: FlattenedPathSink + ?Sized,
{
    let mut current = None;
    let mut contour_start = None;
    let mut contour_active = false;

    for command in path.commands() {
        match *command {
            PathCommand::MoveTo(point) => {
                if contour_active {
                    sink.end_contour(false);
                }

                let point = transform.map_point(point);

                sink.begin_contour(point);

                current = Some(point);
                contour_start = Some(point);
                contour_active = true;
            }

            PathCommand::LineTo(point) => {
                let Some(from) = current else {
                    continue;
                };

                let to = transform.map_point(point);

                if from != to {
                    sink.segment(from, to);
                }

                current = Some(to);
            }

            PathCommand::QuadraticTo { control, to } => {
                let Some(from) = current else {
                    continue;
                };

                let control = transform.map_point(control);
                let to = transform.map_point(to);

                flatten_quadratic(sink, from, control, to);

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

                flatten_cubic(sink, from, control_1, control_2, to);

                current = Some(to);
            }

            PathCommand::Close => {
                let (Some(from), Some(start)) = (current, contour_start) else {
                    continue;
                };

                if from != start {
                    sink.segment(from, start);
                }

                sink.end_contour(true);

                current = None;
                contour_start = None;
                contour_active = false;
            }
        }
    }

    if contour_active {
        sink.end_contour(false);
    }
}

fn flatten_quadratic<S>(sink: &mut S, start: VectorPoint, control: VectorPoint, end: VectorPoint)
where
    S: FlattenedPathSink + ?Sized,
{
    let steps = curve_steps(&[start, control, end]);

    let mut previous = start;

    for step in 1..=steps {
        let t = step as f32 / steps as f32;
        let inverse = 1.0 - t;

        let point = VectorPoint::new(
            inverse * inverse * start.x + 2.0 * inverse * t * control.x + t * t * end.x,
            inverse * inverse * start.y + 2.0 * inverse * t * control.y + t * t * end.y,
        );

        if previous != point {
            sink.segment(previous, point);
        }

        previous = point;
    }
}

fn flatten_cubic<S>(
    sink: &mut S,
    start: VectorPoint,
    control_1: VectorPoint,
    control_2: VectorPoint,
    end: VectorPoint,
) where
    S: FlattenedPathSink + ?Sized,
{
    let steps = curve_steps(&[start, control_1, control_2, end]);

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

        if previous != point {
            sink.segment(previous, point);
        }

        previous = point;
    }
}

fn curve_steps(points: &[VectorPoint]) -> usize {
    if points.len() < 2 {
        return 1;
    }

    let mut length = 0.0;

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

fn approximate_distance(from: VectorPoint, to: VectorPoint) -> f32 {
    (to.x - from.x).abs() + (to.y - from.y).abs()
}

pub(super) fn ceil_to_i32(value: f32) -> i32 {
    let truncated = value as i32;

    if (truncated as f32) < value {
        truncated.saturating_add(1)
    } else {
        truncated
    }
}

pub(super) fn round_to_i32(value: f32) -> i32 {
    if !value.is_finite() {
        return 0;
    }

    if value >= 0.0 {
        (value + 0.5) as i32
    } else {
        (value - 0.5) as i32
    }
}

pub(super) fn floor_to_i32(value: f32) -> i32 {
    if !value.is_finite() {
        return 0;
    }

    let truncated = value as i32;

    if (truncated as f32) > value {
        truncated.saturating_sub(1)
    } else {
        truncated
    }
}
