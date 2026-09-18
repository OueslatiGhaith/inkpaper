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

#[cfg(feature = "ui-metrics")]
pub(crate) fn log_text_metrics(
    paint: inkpaper_ui::backend::EInkPaintReport,
    cache: inkpaper_ui::GlyphCacheMetrics,
    bytes_used: usize,
    bytes_capacity: usize,
) {
    let bytes_used = u64::try_from(bytes_used).unwrap_or(u64::MAX);
    let bytes_peak = u64::try_from(cache.bytes_peak).unwrap_or(u64::MAX);
    let bytes_capacity = u64::try_from(bytes_capacity).unwrap_or(u64::MAX);

    defmt::info!(
        "perf/text draws={=u64} glyphs={=u64} lookups={=u64} hits={=u64} misses={=u64} collisions={=u64} rasterized={=u64} clears={=u64} bytes_used={=u64} bytes_peak={=u64} bytes_capacity={=u64}",
        paint.text_draw_calls(),
        paint.shaped_glyphs(),
        cache.lookups,
        cache.hits,
        cache.misses,
        cache.collisions,
        cache.rasterizations,
        cache.clears,
        bytes_used,
        bytes_peak,
        bytes_capacity,
    );
}

#[cfg(feature = "ui-metrics")]
pub(crate) fn log_coverage_metrics(
    paint: inkpaper_ui::backend::EInkPaintReport,
    framebuffer_draw_iter_pixels: u64,
) {
    defmt::info!(
        "perf/coverage bitmaps={=u64} samples={=u64} accepted={=u64} framebuffer_pixels={=u64}",
        paint.coverage_bitmaps(),
        paint.coverage_samples(),
        paint.coverage_accepted(),
        framebuffer_draw_iter_pixels,
    );
}

#[cfg(feature = "performance")]
pub(crate) fn log_ordered_coverage(calls: u64, pixels: u64, cycles: u64) {
    info!(
        "perf/coverage_fast calls={=u64} pixels={=u64} cycles={=u64}",
        calls, pixels, cycles,
    );
}
