mod support;

use inkpaper_ui::{
    DamageRegion, Invalidation, Offset, PerformanceMetrics, Point, Rect, RenderInvalidation, px,
};

use support::{BenchScenario, NESTED_SCROLL_DEPTHS, SCROLL_LIST_SIZES, VIEWPORT, setup};

fn invalidation_name(invalidation: Invalidation) -> &'static str {
    match invalidation {
        Invalidation::None => "none",
        Invalidation::Paint => "paint",
        Invalidation::Layout => "layout",
        Invalidation::Rebuild => "rebuild",
    }
}

fn rect_area(rect: Rect) -> u64 {
    let width = u64::try_from(rect.width().non_negative().get()).unwrap_or(0);
    let height = u64::try_from(rect.height().non_negative().get()).unwrap_or(0);

    width.saturating_mul(height)
}

fn viewport_rect() -> Rect {
    Rect::new(Point::ZERO, VIEWPORT)
}

fn damage_area_within_viewport(damage: DamageRegion) -> u64 {
    let viewport = viewport_rect();
    if damage.is_full() {
        return rect_area(viewport);
    }

    damage
        .rects()
        .iter()
        .filter_map(|rect| rect.intersection(viewport))
        .map(rect_area)
        .fold(0u64, u64::saturating_add)
}

fn coverage_basis_points(area: u64) -> u64 {
    let viewport_area = rect_area(viewport_rect());
    if viewport_area == 0 {
        return 0;
    }

    area.saturating_mul(10_000) / viewport_area
}

fn print_result(
    operation: &str,
    scenario: BenchScenario,
    size: usize,
    invalidation: RenderInvalidation,
    metrics: PerformanceMetrics,
) {
    let damage = invalidation.damage();
    let area = damage_area_within_viewport(damage);

    println!(
        "{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
        operation,
        scenario.name(),
        size,
        invalidation_name(invalidation.kind(),),
        damage.is_full(),
        damage.len(),
        area,
        coverage_basis_points(area),
        metrics.render_invalidations_consumed,
        metrics.full_damage_invalidations,
        metrics.partial_damage_invalidations,
        metrics.damage_rectangles,
        metrics.visual_traversal_passes,
        metrics.visual_traversal_nodes,
    );
}

fn main() {
    println!(
        "operation,\
scenario,\
size,\
kind,\
full_damage,\
damage_rects,\
damage_pixels,\
coverage_basis_points,\
render_invalidations_consumed,\
full_damage_invalidations,\
partial_damage_invalidations,\
damage_rectangles_recorded,\
visual_traversal_passes,\
visual_traversal_nodes"
    );

    for &depth in NESTED_SCROLL_DEPTHS {
        let (mut runtime, _, _) = setup(BenchScenario::NestedScrollFocus, depth);

        runtime.reset_performance_metrics();

        assert!(
            runtime.focus_next(),
            "nested-scroll scenario must contain a focusable target",
        );

        let invalidation = runtime.take_render_invalidation();
        let metrics = runtime.performance_metrics();

        print_result(
            "focus_next",
            BenchScenario::NestedScrollFocus,
            depth,
            invalidation,
            metrics,
        );
    }

    for &rows in SCROLL_LIST_SIZES {
        let (mut runtime, _, _) = setup(BenchScenario::ScrollList, rows);

        runtime.reset_performance_metrics();

        assert!(
            runtime.scroll_at(Point::new(px(10), px(10),), Offset::new(px(0), px(16),),),
            "scroll-list scenario must be scrollable",
        );

        let invalidation = runtime.take_render_invalidation();
        let metrics = runtime.performance_metrics();

        print_result(
            "scroll_at",
            BenchScenario::ScrollList,
            rows,
            invalidation,
            metrics,
        );
    }
}
