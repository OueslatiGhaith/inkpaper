use crate::raster::{flatten_cubic, flatten_quadratic};
use crate::{AffineTransform, PathCommand, VectorPath, VectorPoint};

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

                flatten_quadratic(from, control, to, |a, b| sink.segment(a, b));

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

                flatten_cubic(from, control_1, control_2, to, |a, b| sink.segment(a, b));

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
