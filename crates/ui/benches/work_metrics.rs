mod support;

use support::{ALL_CASES, VIEWPORT, setup};

fn main() {
    println!(
        "scenario,size,\
entity_renders,\
nodes_mounted,\
measure_calls,\
measurement_cache_hits,\
measurement_cache_misses,\
text_measurements,\
flex_base_calls,\
flex_item_calls,\
flex_sibling_visits,\
nodes_laid_out,\
visual_nodes_visited,\
visible_nodes,\
nodes_painted"
    );

    for &(scenario, sizes) in ALL_CASES {
        for &size in sizes {
            let (mut runtime, app, mut painter) = setup(scenario, size);

            runtime.reset_performance_metrics();

            runtime.rebuild(app).unwrap();

            let rebuild = runtime.performance_metrics();

            runtime.reset_performance_metrics();

            runtime.layout(VIEWPORT, &painter).unwrap();

            let layout = runtime.performance_metrics();

            runtime.reset_performance_metrics();

            runtime.paint(&mut painter).unwrap();

            let paint = runtime.performance_metrics();

            println!(
                "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
                scenario.name(),
                size,
                rebuild.entity_render_calls,
                rebuild.nodes_mounted,
                layout.measure_node_calls,
                layout.measurement_cache_hits,
                layout.measurement_cache_misses,
                layout.text_measurements,
                layout.flex_base_main_size_calls,
                layout.flex_item_main_size_calls,
                layout.flex_sibling_visits,
                layout.nodes_laid_out,
                paint.visual_nodes_visited,
                paint.visible_nodes,
                paint.nodes_painted,
            );
        }
    }
}
