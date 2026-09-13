use crate::{
    AffineTransform, CanvasPainter, Color, PaintReport, Painter, PathFill, PathStroke, Pixels,
    Rect, ResolvedTextStyle, SvgSource, SvgViewBox, px,
};

pub(super) fn paint_svg<P>(
    source: SvgSource,
    bounds: Rect,
    clip: Option<Rect>,
    text_style: ResolvedTextStyle,
    report: &mut PaintReport,
    painter: &mut P,
) -> Result<(), P::Error>
where
    P: Painter,
{
    if !bounds.has_area() {
        return Ok(());
    }

    let current_color = text_style.color;

    let mut draw = |local_bounds: Rect, canvas: &mut dyn CanvasPainter| {
        draw_svg(source, local_bounds, current_color, canvas);
    };

    painter.draw_canvas(bounds, clip, &mut draw)?;

    if source.has_paint() {
        report.mark_graphics();
    }

    Ok(())
}

fn draw_svg(
    source: SvgSource,
    bounds: Rect,
    current_color: Color,
    painter: &mut dyn CanvasPainter,
) {
    let Some((transform, scale)) = view_box_transform(source.view_box(), bounds) else {
        return;
    };

    for path in source.paths() {
        if let Some(fill) = path.fill {
            painter.fill_path(
                path.path,
                transform,
                PathFill::new(fill.paint.resolve(current_color)).with_rule(fill.rule),
            );
        }

        if let Some(stroke) = path.stroke {
            let width = scaled_stroke_width(stroke.width, scale);

            if width.is_non_positive() {
                continue;
            }

            painter.stroke_path(
                path.path,
                transform,
                PathStroke::new(width, stroke.paint.resolve(current_color))
                    .with_cap(stroke.cap)
                    .with_join(stroke.join)
                    .with_miter_limit(stroke.miter_limit),
            );
        }
    }
}

fn view_box_transform(view_box: SvgViewBox, bounds: Rect) -> Option<(AffineTransform, f32)> {
    if !view_box.width.is_finite()
        || !view_box.height.is_finite()
        || view_box.width <= 0.0
        || view_box.height <= 0.0
        || bounds.width().is_non_positive()
        || bounds.height().is_non_positive()
    {
        return None;
    }

    let bounds_width = bounds.width().get() as f32;
    let bounds_height = bounds.height().get() as f32;

    let scale_x = bounds_width / view_box.width;
    let scale_y = bounds_height / view_box.height;

    let scale = if scale_x < scale_y { scale_x } else { scale_y };

    if !scale.is_finite() || scale <= 0.0 {
        return None;
    }

    let rendered_width = view_box.width * scale;
    let rendered_height = view_box.height * scale;

    let offset_x = (bounds_width - rendered_width) / 2.0;

    let offset_y = (bounds_height - rendered_height) / 2.0;

    let translate_x = bounds.x().get() as f32 + offset_x - view_box.min_x * scale;

    let translate_y = bounds.y().get() as f32 + offset_y - view_box.min_y * scale;

    Some((
        AffineTransform::scale_translate(scale, scale, translate_x, translate_y),
        scale,
    ))
}

fn scaled_stroke_width(width: f32, scale: f32) -> Pixels {
    if !width.is_finite() || !scale.is_finite() || width <= 0.0 || scale <= 0.0 {
        return px(0);
    }

    let scaled = width * scale;

    if !scaled.is_finite() || scaled <= 0.0 {
        return px(0);
    }

    if scaled >= i32::MAX as f32 {
        return px(i32::MAX);
    }

    let rounded = (scaled + 0.5) as i32;

    px(rounded.max(1))
}

#[cfg(test)]
mod tests {
    use std::vec::Vec;

    use crate::{
        Color, PathCommand, Pixels, Point, Size, StrokeCap, StrokeJoin, SvgPaint, SvgPath,
        SvgStroke, VectorPath, VectorPoint,
    };

    use super::*;

    #[derive(Default)]
    struct RecordingPainter {
        lines: Vec<(Point, Point, Pixels, Color)>,
    }

    impl CanvasPainter for RecordingPainter {
        fn fill_rect(&mut self, _: Rect, _: Color) {}

        fn stroke_rect(&mut self, _: Rect, _: Pixels, _: Color) {}

        fn line(&mut self, start: Point, end: Point, width: Pixels, color: Color) {
            self.lines.push((start, end, width, color));
        }

        fn fill_circle(&mut self, _: Point, _: Pixels, _: Color) {}

        fn stroke_circle(&mut self, _: Point, _: Pixels, _: Pixels, _: Color) {}
    }

    #[test]
    fn svg_preserves_view_box_aspect_ratio_and_resolves_current_color() {
        static COMMANDS: [PathCommand; 2] = [
            PathCommand::MoveTo(VectorPoint::new(0.0, 0.0)),
            PathCommand::LineTo(VectorPoint::new(10.0, 0.0)),
        ];

        static PATHS: [SvgPath; 1] = [SvgPath::new(VectorPath::new(&COMMANDS)).with_stroke(
            SvgStroke::new(1.0, SvgPaint::CurrentColor)
                .with_cap(StrokeCap::Butt)
                .with_join(StrokeJoin::Miter),
        )];

        let source = SvgSource::new(
            Size::new(px(10), px(5)),
            SvgViewBox::new(0.0, 0.0, 10.0, 5.0),
            &PATHS,
        );

        let mut painter = RecordingPainter::default();

        draw_svg(
            source,
            Rect::new(Point::ZERO, Size::new(px(20), px(20))),
            Color::RED,
            &mut painter,
        );

        assert_eq!(painter.lines.len(), 1);

        assert_eq!(
            painter.lines[0],
            (
                Point::new(px(0), px(5)),
                Point::new(px(20), px(5)),
                px(2),
                Color::RED,
            ),
        );
    }
}
