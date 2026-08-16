mod support;

use inkpaper_ui::{Offset, Pixels, Point, px};

use support::{
    BenchScenario, DEEP_SCROLL_LIST_SIZES, SCROLL_LIST_ROW_HEIGHT, scroll_list_max_offset, setup,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScrollDepth {
    Top,
    Quarter,
    Half,
    ThreeQuarters,
    NearBottom,
}

impl ScrollDepth {
    const ALL: &[Self] = &[
        Self::Top,
        Self::Quarter,
        Self::Half,
        Self::ThreeQuarters,
        Self::NearBottom,
    ];

    const fn name(self) -> &'static str {
        match self {
            Self::Top => "top",
            Self::Quarter => "25%",
            Self::Half => "50%",
            Self::ThreeQuarters => "75%",
            Self::NearBottom => "near_bottom",
        }
    }

    fn target_offset(self, maximum: Pixels) -> Pixels {
        match self {
            Self::Top => SCROLL_LIST_ROW_HEIGHT.min(maximum),
            Self::Quarter => maximum / 4,
            Self::Half => maximum / 2,
            Self::ThreeQuarters => maximum * 3 / 4,
            // leave one row of scroll range after the target. This gives us a very
            // deep prefix while still being distinct from the exact clamp boundary.
            Self::NearBottom => (maximum - SCROLL_LIST_ROW_HEIGHT).non_negative(),
        }
    }
}

fn main() {
    println!(
        "scenario,\
rows,\
depth,\
target_offset_px,\
max_offset_px,\
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
damage_clip_paints,\
draw_calls"
    );

    for &rows in DEEP_SCROLL_LIST_SIZES {
        let maximum = scroll_list_max_offset(rows);

        assert!(
            maximum.is_positive(),
            "deep-scroll benchmark requires a scrollable list",
        );

        for &depth in ScrollDepth::ALL {
            // each sample receives a fresh Runtime, so the scroll delta is also
            // the absolute target offset from zero.
            let (mut runtime, _, mut painter) = setup(BenchScenario::ScrollList, rows);

            let target = depth.target_offset(maximum);

            assert!(
                target.is_positive(),
                "deep-scroll target must move the list",
            );
            assert!(
                runtime.scroll_at(Point::new(px(10), px(10),), Offset::new(px(0), target,),),
                "scroll-list scenario must reach the requested offset",
            );

            let invalidation = runtime.take_render_invalidation();

            assert!(
                !invalidation.damage().is_none(),
                "scrolling must produce paint damage",
            );

            runtime.reset_performance_metrics();

            runtime
                .paint_with_damage(invalidation.damage(), &mut painter)
                .unwrap()
                .unwrap();

            let metrics = runtime.performance_metrics();

            println!(
                "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
                BenchScenario::ScrollList.name(),
                rows,
                depth.name(),
                target.get(),
                maximum.get(),
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
                metrics.damage_clip_paints,
                painter.draw_calls(),
            );
        }
    }
}
