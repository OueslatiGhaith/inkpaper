#[cfg(any(feature = "performance", feature = "trace"))]
use defmt::info;
#[cfg(feature = "performance")]
use embedded_hal_async::delay::DelayNs;
#[cfg(feature = "performance")]
use epd_bus::{BusyPolarity, EpdInterface};
#[cfg(feature = "performance")]
use esp_hal::xtensa_lx::timer::get_cycle_count;
#[cfg(feature = "performance")]
use inkpaper_trace::{TraceEvent, profile_expr};

#[cfg(feature = "performance")]
use crate::firmware::presenter::FrameUpdate;

pub(crate) const CLOCK_HZ: u32 = 240_000_000;

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
pub(crate) struct PresentTimings {
    total_cycles: u32,
    io_cycles: u64,
    busy_cycles: u64,
    io_bytes: u64,
    io_calls: u32,
    busy_waits: u32,
    longest_busy_cycles: u32,
}

#[cfg(feature = "performance")]
impl PresentTimings {
    pub(crate) const fn total_cycles(self) -> u32 {
        self.total_cycles
    }

    pub(crate) const fn io_cycles(self) -> u64 {
        self.io_cycles
    }

    pub(crate) const fn busy_cycles(self) -> u64 {
        self.busy_cycles
    }

    pub(crate) const fn io_bytes(self) -> u64 {
        self.io_bytes
    }

    pub(crate) const fn io_calls(self) -> u32 {
        self.io_calls
    }

    pub(crate) const fn busy_waits(self) -> u32 {
        self.busy_waits
    }

    pub(crate) const fn longest_busy_cycles(self) -> u32 {
        self.longest_busy_cycles
    }

    pub(crate) fn other_cycles(self) -> u64 {
        u64::from(self.total_cycles)
            .saturating_sub(self.io_cycles)
            .saturating_sub(self.busy_cycles)
    }

    fn record_io(&mut self, cycles: u32, bytes: u64) {
        self.io_cycles = self.io_cycles.saturating_add(u64::from(cycles));

        self.io_bytes = self.io_bytes.saturating_add(bytes);

        self.io_calls = self.io_calls.saturating_add(1);
    }

    fn record_busy(&mut self, cycles: u32, wait: bool) {
        self.busy_cycles = self.busy_cycles.saturating_add(u64::from(cycles));

        if wait {
            self.busy_waits = self.busy_waits.saturating_add(1);

            self.longest_busy_cycles = self.longest_busy_cycles.max(cycles);
        }
    }
}

#[cfg(feature = "performance")]
pub(crate) struct ProfiledEpdBus<'a, B> {
    inner: &'a mut B,
    timings: PresentTimings,
}

#[cfg(feature = "performance")]
impl<'a, B> ProfiledEpdBus<'a, B> {
    pub(crate) fn new(inner: &'a mut B) -> Self {
        Self {
            inner,
            timings: PresentTimings::default(),
        }
    }

    pub(crate) fn finish(mut self, total_cycles: u32) -> PresentTimings {
        self.timings.total_cycles = total_cycles;

        self.timings
    }

    #[inline(always)]
    fn record_io(&mut self, cycles: u32, bytes: u64) {
        self.timings.record_io(cycles, bytes);
    }

    #[inline(always)]
    fn record_busy(&mut self, cycles: u32, wait: bool) {
        self.timings.record_busy(cycles, wait);
    }
}

#[cfg(feature = "performance")]
impl<B> EpdInterface for ProfiledEpdBus<'_, B>
where
    B: EpdInterface,
{
    type Error = B::Error;

    async fn command(&mut self, command: u8) -> Result<(), Self::Error> {
        let timer = CycleTimer::start();

        let result = self.inner.command(command).await;

        self.record_io(timer.elapsed(), 1);

        result
    }

    async fn data(&mut self, data: &[u8]) -> Result<(), Self::Error> {
        let timer = CycleTimer::start();

        let result = self.inner.data(data).await;

        self.record_io(timer.elapsed(), usize_to_u64(data.len()));

        result
    }

    async fn command_data(&mut self, command: u8, data: &[u8]) -> Result<(), Self::Error> {
        let timer = CycleTimer::start();

        let result = self.inner.command_data(command, data).await;

        let bytes = 1u64.saturating_add(usize_to_u64(data.len()));

        self.record_io(timer.elapsed(), bytes);

        result
    }

    async fn reset<D>(&mut self, delay: &mut D) -> Result<(), Self::Error>
    where
        D: DelayNs,
    {
        let timer = CycleTimer::start();

        let result = self.inner.reset(delay).await;

        self.record_io(timer.elapsed(), 0);

        result
    }

    fn is_busy(&mut self, polarity: BusyPolarity) -> Result<bool, Self::Error> {
        let timer = CycleTimer::start();

        let result = self.inner.is_busy(polarity);

        self.record_busy(timer.elapsed(), false);

        result
    }

    async fn wait_busy<D>(
        &mut self,
        polarity: BusyPolarity,
        delay: &mut D,
    ) -> Result<(), Self::Error>
    where
        D: DelayNs,
    {
        let timer = CycleTimer::start();

        let result = profile_expr!(
            TraceEvent::PresentBusy,
            self.inner.wait_busy(polarity, delay,).await,
        );

        self.record_busy(timer.elapsed(), true);

        result
    }

    async fn begin_data_stream(&mut self) -> Result<(), Self::Error> {
        let timer = CycleTimer::start();

        let result = self.inner.begin_data_stream().await;

        self.record_io(timer.elapsed(), 0);

        result
    }

    async fn stream_data(&mut self, data: &[u8]) -> Result<(), Self::Error> {
        let timer = CycleTimer::start();

        let result = self.inner.stream_data(data).await;

        self.record_io(timer.elapsed(), usize_to_u64(data.len()));

        result
    }

    fn end_data_stream(&mut self) -> Result<(), Self::Error> {
        let timer = CycleTimer::start();

        let result = self.inner.end_data_stream();

        self.record_io(timer.elapsed(), 0);

        result
    }
}

#[cfg(feature = "performance")]
fn usize_to_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FramePerfReport {
    render: RenderTimings,
    framebuffer_pixels: u64,
    framebuffer_pixels_valid: bool,
}

#[cfg(feature = "performance")]
impl FramePerfReport {
    pub(crate) const EMPTY: Self = Self {
        render: RenderTimings {
            rebuild_cycles: 0,
            layout_cycles: 0,
            clear_cycles: 0,
            paint_cycles: 0,
            damage_cycles: 0,
        },
        framebuffer_pixels: 0,
        framebuffer_pixels_valid: false,
    };

    pub(crate) const fn new(render: RenderTimings) -> Self {
        Self {
            render,
            framebuffer_pixels: 0,
            framebuffer_pixels_valid: false,
        }
    }

    pub(crate) const fn with_framebuffer_pixels(mut self, framebuffer_pixels: u64) -> Self {
        self.framebuffer_pixels = framebuffer_pixels;
        self.framebuffer_pixels_valid = true;
        self
    }

    pub(crate) const fn render(self) -> RenderTimings {
        self.render
    }

    pub(crate) const fn framebuffer_pixels(self) -> u64 {
        self.framebuffer_pixels
    }

    pub(crate) const fn has_framebuffer_pixels(self) -> bool {
        self.framebuffer_pixels_valid
    }
}

#[cfg(feature = "performance")]
pub(crate) fn log_config() {
    info!("perf/config hz={=u32}", CLOCK_HZ);
}

#[cfg(feature = "performance")]
pub(crate) fn log_render(frame_id: u32, timings: RenderTimings) {
    info!(
        "perf/render frame={=u32} rebuild={=u32} layout={=u32} clear={=u32} paint={=u32} damage={=u32}",
        frame_id,
        timings.rebuild_cycles,
        timings.layout_cycles,
        timings.clear_cycles,
        timings.paint_cycles,
        timings.damage_cycles,
    );
}

#[cfg(feature = "performance")]
pub(crate) fn log_present(frame_id: u32, timings: PresentTimings) {
    info!(
        "perf/present frame={=u32} cycles={=u32} io={=u64} busy={=u64} other={=u64} bytes={=u64} io_calls={=u32} busy_waits={=u32} longest_busy={=u32}",
        frame_id,
        timings.total_cycles(),
        timings.io_cycles(),
        timings.busy_cycles(),
        timings.other_cycles(),
        timings.io_bytes(),
        timings.io_calls(),
        timings.busy_waits(),
        timings.longest_busy_cycles(),
    );
}

#[cfg(feature = "performance")]
pub(crate) fn log_frame(update: FrameUpdate, present: PresentTimings) {
    let report = update.perf_report();
    let timings = report.render();
    let damage = update.physical_damage();
    let eink = update.eink_report();

    info!(
        "perf/frame id={=u32} refresh={:?} presentation={:?} x={=u16} y={=u16} width={=u16} height={=u16} rebuild={=u32} layout={=u32} clear={=u32} paint={=u32} damage={=u32} present={=u32} present_io={=u64} present_busy={=u64} present_other={=u64} present_bytes={=u64} present_io_calls={=u32} present_busy_waits={=u32} present_longest_busy={=u32} framebuffer_pixels={=u64} framebuffer_valid={=u8} text_draws={=u64} glyphs={=u64}",
        update.frame_id(),
        update.refresh(),
        update.presentation(),
        damage.x,
        damage.y,
        damage.width,
        damage.height,
        timings.rebuild_cycles,
        timings.layout_cycles,
        timings.clear_cycles,
        timings.paint_cycles,
        timings.damage_cycles,
        present.total_cycles(),
        present.io_cycles(),
        present.busy_cycles(),
        present.other_cycles(),
        present.io_bytes(),
        present.io_calls(),
        present.busy_waits(),
        present.longest_busy_cycles(),
        report.framebuffer_pixels(),
        u8::from(report.has_framebuffer_pixels(),),
        eink.text_draw_calls(),
        eink.shaped_glyphs(),
    );
}

#[cfg(feature = "ui-metrics")]
pub(crate) fn log_ui_metrics(frame_id: u32, metrics: inkpaper_ui::PerformanceMetrics) {
    defmt::info!(
        "perf/ui frame={=u32} render_calls={=u64} mounted={=u64} measure={=u64} cache_hits={=u64} cache_misses={=u64} text_measurements={=u64} laid_out={=u64} painted={=u64}",
        frame_id,
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
        "perf/damage frame={=u32} invalidations={=u64} full={=u64} partial={=u64} rects={=u64} culled={=u64} pruned={=u64} extent_pruned={=u64}",
        frame_id,
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
    frame_id: u32,
    paint: inkpaper_ui::backend::EInkPaintReport,
    cache: inkpaper_ui::GlyphCacheMetrics,
    bytes_used: usize,
    bytes_capacity: usize,
) {
    let bytes_used = u64::try_from(bytes_used).unwrap_or(u64::MAX);

    let bytes_peak = u64::try_from(cache.bytes_peak).unwrap_or(u64::MAX);

    let bytes_capacity = u64::try_from(bytes_capacity).unwrap_or(u64::MAX);

    defmt::info!(
        "perf/text frame={=u32} draws={=u64} glyphs={=u64} lookups={=u64} hits={=u64} misses={=u64} collisions={=u64} rasterized={=u64} clears={=u64} bytes_used={=u64} bytes_peak={=u64} bytes_capacity={=u64}",
        frame_id,
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
    frame_id: u32,
    paint: inkpaper_ui::backend::EInkPaintReport,
    framebuffer_draw_iter_pixels: u64,
) {
    defmt::info!(
        "perf/coverage frame={=u32} bitmaps={=u64} samples={=u64} accepted={=u64} framebuffer_pixels={=u64}",
        frame_id,
        paint.coverage_bitmaps(),
        paint.coverage_samples(),
        paint.coverage_accepted(),
        framebuffer_draw_iter_pixels,
    );
}

#[cfg(feature = "performance")]
pub(crate) fn log_ordered_coverage(frame_id: u32, calls: u64, pixels: u64, cycles: u64) {
    info!(
        "perf/coverage_fast frame={=u32} calls={=u64} pixels={=u64} cycles={=u64}",
        frame_id, calls, pixels, cycles,
    );
}

#[cfg(feature = "trace")]
pub(crate) fn log_trace(frame_id: u32, summary: inkpaper_trace::TraceSummary) {
    let spans = u32::try_from(summary.records()).unwrap_or(u32::MAX);

    info!(
        "trace/session id={=u32} frame={=u32} hz={=u32} spans={=u32} dropped={=u32} open={=u8}",
        summary.session_id(),
        frame_id,
        CLOCK_HZ,
        spans,
        summary.dropped(),
        summary.open_spans(),
    );

    for index in 0..summary.records() {
        let Some(record) = inkpaper_trace::record(index) else {
            continue;
        };

        info!(
            "trace/span event={=u8} depth={=u8} start={=u32} cycles={=u32} arg={=u32}",
            record.event().id(),
            record.depth(),
            record.start_cycles(),
            record.duration_cycles(),
            record.arg(),
        );
    }
}
