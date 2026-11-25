use crate::float::{ceil_i32, floor_i32};
use crate::raster::CoverageGrid;
use crate::{
    AffineTransform, CanvasPainter, Color, FillRule, PathFill, Point, Rect, Size, VectorPath,
    VectorPoint, px,
};

use super::path::{FlattenedPathSink, flatten_path};

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

    fn is_finite(&self) -> bool {
        self.min_x.is_finite()
            && self.max_x.is_finite()
            && self.min_y.is_finite()
            && self.max_y.is_finite()
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

    if !bounds.initialized || !bounds.is_finite() {
        return;
    }

    let mut start_x = floor_i32(bounds.min_x);
    let mut end_x = ceil_i32(bounds.max_x);

    let mut start_y = floor_i32(bounds.min_y);
    let mut end_y = ceil_i32(bounds.max_y);

    // a path can reach far past what's drawn, so only scan the visible part
    if let Some(clip) = painter.clip_bounds() {
        start_x = start_x.max(clip.x().get());
        end_x = end_x.min(clip.right().get());
        start_y = start_y.max(clip.y().get());
        end_y = end_y.min(clip.bottom().get());
    }

    if end_x <= start_x || end_y <= start_y {
        return;
    }

    let Ok(width) = usize::try_from(end_x.saturating_sub(start_x)) else {
        return;
    };
    let Ok(height) = usize::try_from(end_y.saturating_sub(start_y)) else {
        return;
    };

    #[cfg(any(test, feature = "alloc", feature = "ttf"))]
    paint_banded_alloc(painter, path, transform, fill, start_x, start_y, width, height);

    #[cfg(not(any(test, feature = "alloc", feature = "ttf")))]
    paint_tiled_no_alloc(painter, path, transform, fill, start_x, start_y, width, height);
}

#[cfg(any(test, feature = "alloc", feature = "ttf"))]
struct SegmentCollector {
    min_y: f32,
    max_y: f32,
    contour_start: Option<VectorPoint>,
    contour_end: Option<VectorPoint>,
    segments: alloc::vec::Vec<(VectorPoint, VectorPoint)>,
}

#[cfg(any(test, feature = "alloc", feature = "ttf"))]
impl SegmentCollector {
    fn new(min_y: i32, max_y: i32) -> Self {
        Self {
            min_y: min_y as f32,
            max_y: max_y as f32,
            contour_start: None,
            contour_end: None,
            segments: alloc::vec::Vec::new(),
        }
    }

    fn push(&mut self, from: VectorPoint, to: VectorPoint) {
        if from.y == to.y {
            return;
        }
        let seg_min_y = from.y.min(to.y);
        let seg_max_y = from.y.max(to.y);
        if seg_max_y <= self.min_y || seg_min_y >= self.max_y {
            return;
        }
        self.segments.push((from, to));
    }
}

#[cfg(any(test, feature = "alloc", feature = "ttf"))]
impl FlattenedPathSink for SegmentCollector {
    fn begin_contour(&mut self, start: VectorPoint) {
        self.contour_start = Some(start);
        self.contour_end = Some(start);
    }

    fn segment(&mut self, from: VectorPoint, to: VectorPoint) {
        self.push(from, to);
        self.contour_end = Some(to);
    }

    fn end_contour(&mut self, closed: bool) {
        if !closed
            && let (Some(end), Some(start)) = (self.contour_end, self.contour_start)
            && end != start
        {
            self.push(end, start);
        }

        self.contour_start = None;
        self.contour_end = None;
    }
}

#[cfg(any(test, feature = "alloc", feature = "ttf"))]
#[allow(clippy::too_many_arguments)]
fn paint_banded_alloc<P>(
    painter: &mut P,
    path: VectorPath<'_>,
    transform: AffineTransform,
    fill: PathFill,
    start_x: i32,
    start_y: i32,
    width: usize,
    height: usize,
) where
    P: CanvasPainter + ?Sized,
{
    /// 4096 floats = 16 KiB scratch buffer per sweep.
    const BAND_CELLS: usize = 4096;

    let end_y = start_y.saturating_add(i32::try_from(height).unwrap_or(i32::MAX));
    let mut collector = SegmentCollector::new(start_y, end_y);
    flatten_path(path, transform, &mut collector);

    if collector.segments.is_empty() {
        return;
    }

    let stride = width + 1;
    let band_height = (BAND_CELLS / stride).max(1).min(height);
    let mut cells = alloc::vec![0.0f32; band_height * stride];

    let mut row_offset = 0usize;
    while row_offset < height {
        let rows = (height - row_offset).min(band_height);
        let band_y = start_y.saturating_add(i32::try_from(row_offset).unwrap_or(i32::MAX));

        let mut grid = CoverageGrid::new(&mut cells, width, rows, start_x, band_y);
        for &(from, to) in &collector.segments {
            grid.add_segment(from, to);
        }

        emit_grid_rows(painter, &grid, rows, start_x, band_y, fill.rule, fill.color);

        row_offset += rows;
    }
}

#[cfg(not(any(test, feature = "alloc", feature = "ttf")))]
struct GridSink<'a, 'b> {
    grid: &'a mut CoverageGrid<'b>,
    contour_start: Option<VectorPoint>,
    contour_end: Option<VectorPoint>,
}

#[cfg(not(any(test, feature = "alloc", feature = "ttf")))]
impl<'a, 'b> GridSink<'a, 'b> {
    fn new(grid: &'a mut CoverageGrid<'b>) -> Self {
        Self {
            grid,
            contour_start: None,
            contour_end: None,
        }
    }
}

#[cfg(not(any(test, feature = "alloc", feature = "ttf")))]
impl FlattenedPathSink for GridSink<'_, '_> {
    fn begin_contour(&mut self, start: VectorPoint) {
        self.contour_start = Some(start);
        self.contour_end = Some(start);
    }

    fn segment(&mut self, from: VectorPoint, to: VectorPoint) {
        self.grid.add_segment(from, to);
        self.contour_end = Some(to);
    }

    fn end_contour(&mut self, closed: bool) {
        if !closed
            && let (Some(end), Some(start)) = (self.contour_end, self.contour_start)
            && end != start
        {
            self.grid.add_segment(end, start);
        }

        self.contour_start = None;
        self.contour_end = None;
    }
}

#[cfg(not(any(test, feature = "alloc", feature = "ttf")))]
#[allow(clippy::too_many_arguments)]
fn paint_tiled_no_alloc<P>(
    painter: &mut P,
    path: VectorPath<'_>,
    transform: AffineTransform,
    fill: PathFill,
    start_x: i32,
    start_y: i32,
    width: usize,
    height: usize,
) where
    P: CanvasPainter + ?Sized,
{
    use crate::raster::STACK_CELLS;

    let mut cells = [0.0f32; STACK_CELLS];
    let tile_width = width.min(STACK_CELLS - 1);
    let tile_height = (STACK_CELLS / (tile_width + 1)).max(1).min(height);

    let mut y_offset = 0usize;
    while y_offset < height {
        let rows = (height - y_offset).min(tile_height);
        let tile_y = start_y.saturating_add(i32::try_from(y_offset).unwrap_or(i32::MAX));

        let mut x_offset = 0usize;
        while x_offset < width {
            let cols = (width - x_offset).min(tile_width);
            let tile_x = start_x.saturating_add(i32::try_from(x_offset).unwrap_or(i32::MAX));

            let mut grid = CoverageGrid::new(&mut cells, cols, rows, tile_x, tile_y);
            let mut sink = GridSink::new(&mut grid);
            flatten_path(path, transform, &mut sink);

            emit_grid_rows(painter, &grid, rows, tile_x, tile_y, fill.rule, fill.color);

            x_offset += cols;
        }

        y_offset += rows;
    }
}

fn emit_grid_rows<P>(
    painter: &mut P,
    grid: &CoverageGrid<'_>,
    rows: usize,
    origin_x: i32,
    origin_y: i32,
    rule: FillRule,
    color: Color,
) where
    P: CanvasPainter + ?Sized,
{
    for row in 0..rows {
        let y = origin_y.saturating_add(i32::try_from(row).unwrap_or(i32::MAX));
        let mut full_run_start = None;
        let mut last_x = origin_x;

        grid.resolve_row(row, rule, |col, coverage| {
            let x = origin_x.saturating_add(i32::try_from(col).unwrap_or(i32::MAX));
            last_x = x.saturating_add(1);

            if coverage == u8::MAX {
                if full_run_start.is_none() {
                    full_run_start = Some(x);
                }
            } else {
                flush_full_run(painter, &mut full_run_start, x, y, color);

                if coverage != 0 {
                    painter.fill_pixel_coverage(Point::new(px(x), px(y)), color, coverage);
                }
            }
        });

        flush_full_run(painter, &mut full_run_start, last_x, y, color);
    }
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
        Rect, Size, VectorPath, VectorPoint, px,
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

    /// records fills, and can only draw inside `clip`
    struct ClippedPainter {
        clip: Rect,
        fills: Vec<Rect>,
    }

    impl CanvasPainter for ClippedPainter {
        fn fill_rect(&mut self, rect: Rect, _: Color) {
            self.fills.push(rect);
        }

        fn stroke_rect(&mut self, _: Rect, _: Pixels, _: Color) {}
        fn line(&mut self, _: Point, _: Point, _: Pixels, _: Color) {}
        fn fill_circle(&mut self, _: Point, _: Pixels, _: Color) {}
        fn stroke_circle(&mut self, _: Point, _: Pixels, _: Pixels, _: Color) {}

        fn clip_bounds(&self) -> Option<Rect> {
            Some(self.clip)
        }
    }

    #[test]
    fn fills_only_scan_the_painters_clip() {
        const COMMANDS: [PathCommand; 5] = [
            PathCommand::MoveTo(VectorPoint::new(-100_000.0, 0.0)),
            PathCommand::LineTo(VectorPoint::new(100_000.0, 0.0)),
            PathCommand::LineTo(VectorPoint::new(100_000.0, 4.0)),
            PathCommand::LineTo(VectorPoint::new(-100_000.0, 4.0)),
            PathCommand::Close,
        ];

        let mut painter = ClippedPainter {
            clip: Rect::new(Point::new(px(10), px(1)), Size::new(px(6), px(2))),
            fills: Vec::new(),
        };

        painter.fill_path(
            VectorPath::new(&COMMANDS),
            AffineTransform::IDENTITY,
            PathFill::new(Color::BLACK),
        );

        let row = |y| Rect::new(Point::new(px(10), px(y)), Size::new(px(6), px(1)));

        assert_eq!(painter.fills, vec![row(1), row(2)]);
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
