#[cfg(feature = "performance")]
use defmt::info;
#[cfg(feature = "performance")]
use esp_hal::xtensa_lx::timer::get_cycle_count;

#[cfg(feature = "performance")]
#[derive(Debug, Clone, Copy)]
pub(crate) struct CycleTimer {
    started_at: u32,
}

#[cfg(feature = "performance")]
impl CycleTimer {
    #[inline(always)]
    pub(crate) fn start() -> Self {
        Self {
            started_at: get_cycle_count(),
        }
    }

    #[inline(always)]
    pub(crate) fn elapsed(self) -> u32 {
        get_cycle_count().wrapping_sub(self.started_at)
    }
}

#[cfg(feature = "performance")]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RenderTimings {
    pub(crate) rebuild_cycles: u32,
    pub(crate) layout_cycles: u32,
    pub(crate) clear_cycles: u32,
    pub(crate) paint_cycles: u32,
    pub(crate) damage_cycles: u32,
}

#[cfg(feature = "performance")]
pub(crate) fn log_render(timings: RenderTimings) {
    info!(
        "perf/render rebuild={=u32} layout={=u32} clear={=u32} paint={=u32} damage={=u32}",
        timings.rebuild_cycles,
        timings.layout_cycles,
        timings.clear_cycles,
        timings.paint_cycles,
        timings.damage_cycles,
    );
}

#[cfg(feature = "performance")]
pub(crate) fn log_present(cycles: u32) {
    info!("perf/present cycles={=u32}", cycles);
}

#[cfg(feature = "ui-metrics")]
pub(crate) fn log_ui_metrics(metrics: inkpaper_ui::PerformanceMetrics) {
    defmt::info!(
        "perf/ui render_calls={=u64} mounted={=u64} measure={=u64} cache_hits={=u64} cache_misses={=u64} text_measurements={=u64} laid_out={=u64} painted={=u64}",
        metrics.entity_render_calls,
        metrics.nodes_mounted,
        metrics.measure_node_calls,
        metrics.measurement_cache_hits,
        metrics.measurement_cache_misses,
        metrics.text_measurements,
        metrics.nodes_laid_out,
        metrics.nodes_painted,
    );

    defmt::info!(
        "perf/damage invalidations={=u64} full={=u64} partial={=u64} rects={=u64} culled={=u64} pruned={=u64} extent_pruned={=u64}",
        metrics.render_invalidations_consumed,
        metrics.full_damage_invalidations,
        metrics.partial_damage_invalidations,
        metrics.damage_rectangles,
        metrics.damage_culled_nodes,
        metrics.damage_pruned_subtrees,
        metrics.damage_extent_pruned_subtrees,
    );
}
