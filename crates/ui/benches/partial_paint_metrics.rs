mod support;

use inkpaper_ui::{Offset, Point, px};

use support::{BenchScenario, NESTED_SCROLL_DEPTHS, SCROLL_LIST_SIZES, setup};

fn main() {
    println!(
        "operation,\
scenario,\
size,\
damage_rects,\
visual_traversal_passes,\
visual_traversal_nodes,\
visual_nodes_visited,\
visible_nodes,\
nodes_painted,\
damage_culled_nodes,\
damage_pruned_subtrees,\
damage_extent_pruned_subtrees,\
damage_pruned_sibling_runs,\
damage_pruned_sibling_prefixes,\
damage_prefix_search_steps,\
damage_clip_paints,\
draw_calls"
    );

    for &depth in NESTED_SCROLL_DEPTHS {
        let (mut runtime, mut painter) = setup(BenchScenario::NestedScrollFocus, depth);

        assert!(
            runtime.focus_next(),
            "nested-scroll scenario must contain a focusable target",
        );

        let invalidation = runtime.take_render_invalidation();

        runtime.reset_performance_metrics();
        runtime
            .paint_with_damage(invalidation.damage(), &mut painter)
            .unwrap()
            .unwrap();

        let metrics = runtime.performance_metrics();

        println!(
            "focus_next,{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            BenchScenario::NestedScrollFocus.name(),
            depth,
            invalidation.damage().len(),
            metrics.visual_traversal_passes,
            metrics.visual_traversal_nodes,
            metrics.visual_nodes_visited,
            metrics.visible_nodes,
            metrics.nodes_painted,
            metrics.damage_culled_nodes,
            metrics.damage_pruned_subtrees,
            metrics.damage_extent_pruned_subtrees,
            metrics.damage_pruned_sibling_runs,
            metrics.damage_pruned_sibling_prefixes,
            metrics.damage_prefix_search_steps,
            metrics.damage_clip_paints,
            painter.draw_calls(),
        );
    }

    for &rows in SCROLL_LIST_SIZES {
        let (mut runtime, mut painter) = setup(BenchScenario::ScrollList, rows);

        assert!(
            runtime.scroll_at(Point::new(px(10), px(10),), Offset::new(px(0), px(16),),),
            "scroll-list scenario must be scrollable",
        );

        let invalidation = runtime.take_render_invalidation();

        runtime.reset_performance_metrics();
        runtime
            .paint_with_damage(invalidation.damage(), &mut painter)
            .unwrap()
            .unwrap();

        let metrics = runtime.performance_metrics();

        println!(
            "scroll_at,{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            BenchScenario::ScrollList.name(),
            rows,
            invalidation.damage().len(),
            metrics.visual_traversal_passes,
            metrics.visual_traversal_nodes,
            metrics.visual_nodes_visited,
            metrics.visible_nodes,
            metrics.nodes_painted,
            metrics.damage_culled_nodes,
            metrics.damage_pruned_subtrees,
            metrics.damage_extent_pruned_subtrees,
            metrics.damage_pruned_sibling_runs,
            metrics.damage_pruned_sibling_prefixes,
            metrics.damage_prefix_search_steps,
            metrics.damage_clip_paints,
            painter.draw_calls(),
        );
    }
}
