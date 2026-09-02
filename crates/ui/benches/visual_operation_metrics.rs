mod support;

use inkpaper_ui::{Offset, PerformanceMetrics, Point, px};

use support::{BenchScenario, NESTED_SCROLL_DEPTHS, SCROLL_LIST_SIZES, setup};

fn print_metrics(
    operation: &str,
    scenario: BenchScenario,
    size: usize,
    node_count: usize,
    metrics: PerformanceMetrics,
) {
    println!(
        "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
        operation,
        scenario.name(),
        size,
        node_count,
        metrics.visual_traversal_passes,
        metrics.visual_traversal_nodes,
        metrics.visual_context_derivations,
        metrics.visual_clip_intersections,
        metrics.visual_ancestor_pushes,
        metrics.visual_ancestor_pops,
        metrics.visual_ancestor_depth_peak,
        metrics.visual_context_stack_depth_peak,
        metrics.visual_context_stack_overflows,
        metrics.visual_context_recomputations,
        metrics.visual_bounds_ancestor_visits,
        metrics.scroll_into_view_ancestor_visits,
    );
}

fn main() {
    println!(
        "operation,\
scenario,\
size,\
frame_nodes,\
visual_traversal_passes,\
visual_traversal_nodes,\
visual_context_derivations,\
visual_clip_intersections,\
visual_ancestor_pushes,\
visual_ancestor_pops,\
visual_ancestor_depth_peak,\
visual_context_stack_depth_peak,\
visual_context_stack_overflows,\
visual_context_recomputations,\
visual_bounds_ancestor_visits,\
scroll_into_view_ancestor_visits"
    );

    for &depth in NESTED_SCROLL_DEPTHS {
        let (mut runtime, _) = setup(BenchScenario::NestedScrollFocus, depth);

        let node_count = runtime.frame_node_count();

        runtime.reset_performance_metrics();

        assert!(
            runtime.focus_next(),
            "nested-scroll scenario must contain a focusable target",
        );

        print_metrics(
            "focus_next",
            BenchScenario::NestedScrollFocus,
            depth,
            node_count,
            runtime.performance_metrics(),
        );
    }

    let position = Point::new(px(8), px(8));

    for &rows in SCROLL_LIST_SIZES {
        let (mut runtime, _) = setup(BenchScenario::ScrollList, rows);

        let node_count = runtime.frame_node_count();

        runtime.reset_performance_metrics();
        runtime.scroll_at(position, Offset::ZERO);

        print_metrics(
            "scroll_at",
            BenchScenario::ScrollList,
            rows,
            node_count,
            runtime.performance_metrics(),
        );
    }
}
