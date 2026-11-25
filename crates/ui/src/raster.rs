//! Single-pass analytical-coverage rasterizer shared by TrueType glyph outlines
//! and filled vector paths.
//!
//! Each flattened line segment accumulates its exact signed trapezoidal area
//! delta into a 1-float-per-pixel differential grid (`width + 1` floats per row),
//! then one horizontal prefix sum per row converts the grid into 8-bit coverage
//! for either [`FillRule::NonZero`] or [`FillRule::EvenOdd`].

use crate::{FillRule, VectorPoint, float::ceil_usize};

pub(crate) const MAX_CURVE_STEPS: usize = 32;

/// Stack scratch capacity (2 KiB) used for single-pass glyph rasterization and
/// `no_alloc` path tiling.
#[cfg(any(feature = "ttf", not(any(test, feature = "alloc"))))]
pub(crate) const STACK_CELLS: usize = 512;

/// Flattens a quadratic Bézier curve using Wang's formula at 0.25 px tolerance.
pub(crate) fn flatten_quadratic(
    start: VectorPoint,
    control: VectorPoint,
    end: VectorPoint,
    mut emit: impl FnMut(VectorPoint, VectorPoint),
) {
    let steps = quadratic_steps(start, control, end);
    let mut previous = start;

    for step in 1..=steps {
        let t = step as f32 / steps as f32;
        let inv = 1.0 - t;
        let point = VectorPoint::new(
            inv * inv * start.x + 2.0 * inv * t * control.x + t * t * end.x,
            inv * inv * start.y + 2.0 * inv * t * control.y + t * t * end.y,
        );

        if previous != point {
            emit(previous, point);
        }

        previous = point;
    }
}

/// Flattens a cubic Bézier curve using Wang's formula at 0.25 px tolerance.
pub(crate) fn flatten_cubic(
    start: VectorPoint,
    control_1: VectorPoint,
    control_2: VectorPoint,
    end: VectorPoint,
    mut emit: impl FnMut(VectorPoint, VectorPoint),
) {
    let steps = cubic_steps(start, control_1, control_2, end);
    let mut previous = start;

    for step in 1..=steps {
        let t = step as f32 / steps as f32;
        let inv = 1.0 - t;
        let inv2 = inv * inv;
        let t2 = t * t;
        let point = VectorPoint::new(
            inv2 * inv * start.x
                + 3.0 * inv2 * t * control_1.x
                + 3.0 * inv * t2 * control_2.x
                + t2 * t * end.x,
            inv2 * inv * start.y
                + 3.0 * inv2 * t * control_1.y
                + 3.0 * inv * t2 * control_2.y
                + t2 * t * end.y,
        );

        if previous != point {
            emit(previous, point);
        }

        previous = point;
    }
}

pub(crate) fn quadratic_steps(start: VectorPoint, control: VectorPoint, end: VectorPoint) -> usize {
    let dx = start.x - 2.0 * control.x + end.x;
    let dy = start.y - 2.0 * control.y + end.y;
    let deviation = libm::hypotf(dx, dy);

    steps_from_deviation(deviation)
}

pub(crate) fn cubic_steps(
    start: VectorPoint,
    control_1: VectorPoint,
    control_2: VectorPoint,
    end: VectorPoint,
) -> usize {
    let d1 = libm::hypotf(
        start.x - 2.0 * control_1.x + control_2.x,
        start.y - 2.0 * control_1.y + control_2.y,
    );
    let d2 = libm::hypotf(
        control_1.x - 2.0 * control_2.x + end.x,
        control_1.y - 2.0 * control_2.y + end.y,
    );

    steps_from_deviation(3.0 * d1.max(d2))
}

fn steps_from_deviation(scaled_deviation: f32) -> usize {
    if !scaled_deviation.is_finite() || scaled_deviation <= 0.0 {
        return 1;
    }

    ceil_usize(libm::sqrtf(scaled_deviation)).clamp(1, MAX_CURVE_STEPS)
}

/// Differential analytical coverage grid covering `[origin_x, origin_x + width)`
/// by `[origin_y, origin_y + height)`.
pub(crate) struct CoverageGrid<'a> {
    cells: &'a mut [f32],
    width: usize,
    height: usize,
    stride: usize,
    origin_x: f32,
    origin_y: f32,
}

impl<'a> CoverageGrid<'a> {
    /// Creates a zeroed grid backed by `cells` (must have length `>= height * (width + 1)`).
    pub(crate) fn new(
        cells: &'a mut [f32],
        width: usize,
        height: usize,
        origin_x: i32,
        origin_y: i32,
    ) -> Self {
        let stride = width + 1;
        let len = height * stride;
        let cells = &mut cells[..len];
        cells.fill(0.0);

        Self {
            cells,
            width,
            height,
            stride,
            origin_x: origin_x as f32,
            origin_y: origin_y as f32,
        }
    }

    /// Accumulates one directed line segment in world/pixel (Y-down) coordinates.
    pub(crate) fn add_segment(&mut self, from: VectorPoint, to: VectorPoint) {
        if self.width == 0 || self.height == 0 {
            return;
        }

        let x0 = from.x - self.origin_x;
        let y0 = from.y - self.origin_y;
        let x1 = to.x - self.origin_x;
        let y1 = to.y - self.origin_y;

        if !x0.is_finite() || !y0.is_finite() || !x1.is_finite() || !y1.is_finite() {
            return;
        }

        let (dir, mut x0, mut y0, x1, y1) = if y0 < y1 {
            (1.0f32, x0, y0, x1, y1)
        } else if y1 < y0 {
            (-1.0f32, x1, y1, x0, y0)
        } else {
            return;
        };

        let max_y = self.height as f32;
        if y1 <= 0.0 || y0 >= max_y {
            return;
        }

        let dxdy = (x1 - x0) / (y1 - y0);
        if !dxdy.is_finite() {
            return;
        }

        if y0 < 0.0 {
            x0 -= y0 * dxdy;
            y0 = 0.0;
        }

        let y_end = y1.min(max_y);
        let max_x = self.width as f32;

        while y0 < y_end {
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "y0 is clamped to [0.0, self.height)"
            )]
            let row = (y0 as usize).min(self.height - 1);
            let row_bottom = ((row + 1) as f32).min(y_end);
            let dy = row_bottom - y0;
            let next_x = x0 + dy * dxdy;

            if dy > 0.0 {
                let row_cells = &mut self.cells[row * self.stride..(row + 1) * self.stride];
                accumulate_row_segment(row_cells, self.width, max_x, x0, next_x, dir * dy);
            }

            x0 = next_x;
            y0 = row_bottom;
        }
    }

    /// Resolves every row into a row-major `width * height` 8-bit coverage buffer.
    #[cfg(any(test, feature = "ttf"))]
    pub(crate) fn write_coverage(&self, fill_rule: FillRule, coverage: &mut [u8]) {
        for row in 0..self.height {
            let row_cells = &self.cells[row * self.stride..(row + 1) * self.stride];
            let dst = &mut coverage[row * self.width..(row + 1) * self.width];
            let mut acc = 0.0f32;

            for (cell, out) in row_cells[..self.width].iter().zip(dst.iter_mut()) {
                acc += *cell;
                *out = resolve_coverage(acc, fill_rule);
            }
        }
    }

    /// Streams each pixel `(x, coverage_byte)` of `row` after prefix-summing.
    pub(crate) fn resolve_row(
        &self,
        row: usize,
        fill_rule: FillRule,
        mut emit: impl FnMut(usize, u8),
    ) {
        let row_cells = &self.cells[row * self.stride..(row + 1) * self.stride];
        let mut acc = 0.0f32;

        for (x, cell) in row_cells[..self.width].iter().enumerate() {
            acc += *cell;
            emit(x, resolve_coverage(acc, fill_rule));
        }
    }
}

fn accumulate_row_segment(
    row_cells: &mut [f32],
    width: usize,
    max_x: f32,
    x_start: f32,
    x_end: f32,
    signed_dy: f32,
) {
    let (mut xa, xb) = if x_start <= x_end {
        (x_start, x_end)
    } else {
        (x_end, x_start)
    };

    if xb <= 0.0 {
        // Entirely to the left of column 0: contributes full winding to every column.
        row_cells[0] += signed_dy;
        return;
    }

    if xa >= max_x {
        // Entirely to the right of the grid: contributes nothing inside 0..width.
        return;
    }

    let dx = xb - xa;
    if dx == 0.0 {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "xa is within [0.0, max_x)"
        )]
        let col = xa as usize;
        if col < width {
            let frac = xa - col as f32;
            row_cells[col] += signed_dy * (1.0 - frac);
            row_cells[col + 1] += signed_dy * frac;
        }
        return;
    }

    let dydx = signed_dy / dx;

    if xa < 0.0 {
        // Portion left of x = 0 adds full winding to column 0.
        row_cells[0] += -xa * dydx;
        xa = 0.0;
    }

    let xb_clipped = xb.min(max_x);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "xa is within [0.0, max_x)"
    )]
    let mut col = xa as usize;

    // Fast path: segment stays inside a single pixel column.
    if col < width && xb <= (col + 1) as f32 {
        let seg_dy = (xb - xa) * dydx;
        let xm = 0.5 * (xa + xb) - col as f32;
        row_cells[col] += seg_dy * (1.0 - xm);
        row_cells[col + 1] += seg_dy * xm;
        return;
    }

    let mut cur_x = xa;
    while cur_x < xb_clipped && col < width {
        let next_x = ((col + 1) as f32).min(xb_clipped);
        let seg_dy = (next_x - cur_x) * dydx;
        let xm = 0.5 * (cur_x + next_x) - col as f32;

        row_cells[col] += seg_dy * (1.0 - xm);
        row_cells[col + 1] += seg_dy * xm;

        cur_x = next_x;
        col += 1;
    }
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "coverage is clamped to [0.0, 1.0] before scaling to 0..=255"
)]
fn resolve_coverage(acc: f32, fill_rule: FillRule) -> u8 {
    let abs = acc.abs();
    let coverage = match fill_rule {
        FillRule::NonZero => abs.min(1.0),
        FillRule::EvenOdd => {
            let wrapped = abs % 2.0;
            if wrapped > 1.0 {
                2.0 - wrapped
            } else {
                wrapped
            }
        }
    };

    (coverage.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

/// Adapts a `skrifa` Y-up outline pen straight into a Y-down [`CoverageGrid`]
/// in one single pass.
#[cfg(feature = "ttf")]
pub(crate) struct GlyphOutlinePen<'a, 'b> {
    grid: &'a mut CoverageGrid<'b>,
    current: Option<VectorPoint>,
    contour_start: Option<VectorPoint>,
}

#[cfg(feature = "ttf")]
impl<'a, 'b> GlyphOutlinePen<'a, 'b> {
    pub(crate) fn new(grid: &'a mut CoverageGrid<'b>) -> Self {
        Self {
            grid,
            current: None,
            contour_start: None,
        }
    }

    pub(crate) fn finish(mut self) {
        self.close_open_contour();
    }

    fn close_open_contour(&mut self) {
        if let (Some(current), Some(start)) = (self.current.take(), self.contour_start.take())
            && current != start
        {
            self.grid.add_segment(current, start);
        }
    }
}

#[cfg(feature = "ttf")]
impl skrifa::outline::OutlinePen for GlyphOutlinePen<'_, '_> {
    fn move_to(&mut self, x: f32, y: f32) {
        self.close_open_contour();
        let point = VectorPoint::new(x, -y);
        self.current = Some(point);
        self.contour_start = Some(point);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let to = VectorPoint::new(x, -y);
        if let Some(from) = self.current {
            self.grid.add_segment(from, to);
        }
        self.current = Some(to);
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        let end = VectorPoint::new(x, -y);
        let Some(start) = self.current else {
            self.current = Some(end);
            return;
        };
        let control = VectorPoint::new(cx0, -cy0);

        flatten_quadratic(start, control, end, |from, to| {
            self.grid.add_segment(from, to);
        });

        self.current = Some(end);
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        let end = VectorPoint::new(x, -y);
        let Some(start) = self.current else {
            self.current = Some(end);
            return;
        };
        let control_1 = VectorPoint::new(cx0, -cy0);
        let control_2 = VectorPoint::new(cx1, -cy1);

        flatten_cubic(start, control_1, control_2, end, |from, to| {
            self.grid.add_segment(from, to);
        });

        self.current = Some(end);
    }

    fn close(&mut self) {
        self.close_open_contour();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_zero_winding_fills_rectangle_cleanly() {
        let mut cells = [0.0f32; 64];
        let mut grid = CoverageGrid::new(&mut cells, 6, 1, 0, 0);

        // Downward right edge at x=5, upward left edge at x=1.
        grid.add_segment(VectorPoint::new(5.0, 0.0), VectorPoint::new(5.0, 1.0));
        grid.add_segment(VectorPoint::new(1.0, 1.0), VectorPoint::new(1.0, 0.0));

        let mut coverage = [0u8; 6];
        grid.write_coverage(FillRule::NonZero, &mut coverage);

        assert_eq!(coverage, [0, 255, 255, 255, 255, 0]);
    }

    #[test]
    fn diagonal_triangle_computes_exact_half_pixel_coverage() {
        let mut cells = [0.0f32; 16];
        let mut grid = CoverageGrid::new(&mut cells, 1, 1, 0, 0);

        // Right triangle covering the lower-left half of pixel (0, 0):
        // (0, 0) -> (1, 1) -> (0, 1) -> (0, 0)
        grid.add_segment(VectorPoint::new(0.0, 0.0), VectorPoint::new(1.0, 1.0));
        grid.add_segment(VectorPoint::new(1.0, 1.0), VectorPoint::new(0.0, 1.0));
        grid.add_segment(VectorPoint::new(0.0, 1.0), VectorPoint::new(0.0, 0.0));

        let mut coverage = [0u8; 1];
        grid.write_coverage(FillRule::NonZero, &mut coverage);

        assert_eq!(coverage[0], 128);
    }

    #[test]
    fn curve_subdivision_is_bounded() {
        let steps = quadratic_steps(
            VectorPoint::new(0.0, 0.0),
            VectorPoint::new(10_000.0, 10_000.0),
            VectorPoint::new(20_000.0, 0.0),
        );

        assert_eq!(steps, MAX_CURVE_STEPS);
    }

    #[test]
    fn tiling_preserves_coverage_for_edges_left_of_origin() {
        let mut cells = [0.0f32; 16];
        let mut grid = CoverageGrid::new(&mut cells, 4, 1, 10, 0);

        // Rectangle spanning x = 0..12, tiled starting at origin_x = 10.
        grid.add_segment(VectorPoint::new(0.0, 1.0), VectorPoint::new(0.0, 0.0));
        grid.add_segment(VectorPoint::new(12.0, 0.0), VectorPoint::new(12.0, 1.0));

        let mut coverage = [0u8; 4];
        grid.write_coverage(FillRule::NonZero, &mut coverage);

        assert_eq!(coverage, [255, 255, 0, 0]);
    }
}
