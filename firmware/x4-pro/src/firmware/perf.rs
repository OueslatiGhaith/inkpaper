#[cfg(any(feature = "performance", feature = "trace"))]
use defmt::info;
#[cfg(feature = "performance")]
use embedded_hal_async::delay::DelayNs;
#[cfg(feature = "performance")]
use epd_bus::{BusyPolarity, EpdInterface};
#[cfg(feature = "performance")]
use esp_hal::xtensa_lx::timer::get_cycle_count;
#[cfg(feature = "performance")]
use inkpaper_trace::{DisplayPhase, TraceEvent, profile_expr, profile_span};

#[cfg(feature = "performance")]
use crate::firmware::{
    presenter::{FrameUpdate, PresentationMode},
    refresh_policy::RefreshRequest,
};

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
const PRESENT_BUSY_WAIT_CAPACITY: usize = 8;

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

    stream_cycles: u64,
    stream_bytes: u64,
    stream_calls: u32,

    busy_wait_samples: [u32; PRESENT_BUSY_WAIT_CAPACITY],
    recorded_busy_waits: u8,
    dropped_busy_waits: u32,
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

    pub(crate) const fn stream_cycles(self) -> u64 {
        self.stream_cycles
    }

    pub(crate) const fn stream_bytes(self) -> u64 {
        self.stream_bytes
    }

    pub(crate) const fn stream_calls(self) -> u32 {
        self.stream_calls
    }

    pub(crate) const fn recorded_busy_waits(self) -> u8 {
        self.recorded_busy_waits
    }

    pub(crate) const fn dropped_busy_waits(self) -> u32 {
        self.dropped_busy_waits
    }

    pub(crate) fn busy_wait_cycles(self, index: u8) -> Option<u32> {
        if index >= self.recorded_busy_waits {
            return None;
        }

        self.busy_wait_samples.get(usize::from(index)).copied()
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

        if !wait {
            return;
        }

        let sample_index = self.busy_waits;

        self.busy_waits = self.busy_waits.saturating_add(1);
        self.longest_busy_cycles = self.longest_busy_cycles.max(cycles);

        let Ok(sample_index) = usize::try_from(sample_index) else {
            self.dropped_busy_waits = self.dropped_busy_waits.saturating_add(1);
            return;
        };

        let Some(slot) = self.busy_wait_samples.get_mut(sample_index) else {
            self.dropped_busy_waits = self.dropped_busy_waits.saturating_add(1);
            return;
        };

        *slot = cycles;

        self.recorded_busy_waits = self.recorded_busy_waits.saturating_add(1);
    }

    fn record_stream_io(&mut self, cycles: u32, bytes: u64) {
        self.record_io(cycles, bytes);

        self.stream_cycles = self.stream_cycles.saturating_add(u64::from(cycles));
        self.stream_bytes = self.stream_bytes.saturating_add(bytes);
        self.stream_calls = self.stream_calls.saturating_add(1);
    }
}

#[cfg(feature = "performance")]
pub(crate) struct ProfiledEpdBus<'a, B> {
    inner: &'a mut B,
    timings: PresentTimings,

    trace_plan: Option<DisplayTracePlan>,
    pending_phase: Option<DisplayPhase>,
}

#[cfg(feature = "performance")]
impl<'a, B> ProfiledEpdBus<'a, B> {
    pub(crate) fn new(
        inner: &'a mut B,
        controller: DisplayController,
        update: FrameUpdate,
    ) -> Self {
        Self {
            inner,
            timings: PresentTimings::default(),

            trace_plan: Some(DisplayTracePlan::new(controller, update)),
            pending_phase: None,
        }
    }

    pub(crate) fn new_preparation(inner: &'a mut B) -> Self {
        Self {
            inner,
            timings: PresentTimings::default(),
            trace_plan: None,
            pending_phase: None,
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

    fn observe_command(&mut self, command: u8) {
        let phase = match command {
            EPD_COMMAND_POWER_ON => Some(DisplayPhase::PowerOn),

            EPD_COMMAND_DISPLAY_REFRESH => {
                let phase = self
                    .trace_plan
                    .as_mut()
                    .map(|plan| plan.next_refresh_phase())
                    .unwrap_or(DisplayPhase::Unknown);

                Some(phase)
            }

            EPD_COMMAND_POWER_OFF => Some(DisplayPhase::PowerOff),

            _ => None,
        };

        let Some(phase) = phase else {
            return;
        };

        self.pending_phase = Some(phase);

        #[cfg(feature = "trace")]
        {
            let _ = inkpaper_trace::async_begin(
                TraceEvent::DisplayPhase,
                DISPLAY_ASYNC_TRACE_ID,
                u32::from(phase.id()),
            );
        }
    }

    pub(crate) fn expect_power_off_completion(&mut self) {
        debug_assert!(self.pending_phase.is_none());

        self.pending_phase = Some(DisplayPhase::PowerOff);
    }

    pub(crate) fn expect_power_on_completion(&mut self) {
        debug_assert!(self.pending_phase.is_none());

        self.pending_phase = Some(DisplayPhase::PowerOn);
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

        if result.is_ok() {
            self.observe_command(command);
        }

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

        let ready = match &result {
            Ok(busy) => !*busy,
            Err(_) => false,
        };

        if ready && self.pending_phase == Some(DisplayPhase::PowerOff) {
            self.pending_phase = None;

            #[cfg(feature = "trace")]
            {
                let _ = inkpaper_trace::async_end(TraceEvent::DisplayPhase, DISPLAY_ASYNC_TRACE_ID);
            }
        }

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
        let wait_index = self.timings.busy_waits();

        let phase = self.pending_phase.take().unwrap_or(DisplayPhase::Unknown);

        let timer = CycleTimer::start();

        let phase_trace = profile_span!(TraceEvent::DisplayPhase, arg = phase.id());

        let result = profile_expr!(
            TraceEvent::PresentBusy,
            arg = wait_index,
            self.inner.wait_busy(polarity, delay).await,
        );

        drop(phase_trace);

        // this intentionally runs even if this ProfiledEpdBus did not observe the command
        // which started the operation.
        //
        // that is what lets a future deferred PowerOff begin during one presentation
        // and be completed by a readiness wait in the next.
        #[cfg(feature = "trace")]
        if result.is_ok() {
            let _ = inkpaper_trace::async_end(TraceEvent::DisplayPhase, DISPLAY_ASYNC_TRACE_ID);
        }

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

        self.timings
            .record_stream_io(timer.elapsed(), usize_to_u64(data.len()));

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
const EPD_COMMAND_POWER_OFF: u8 = 0x02;

#[cfg(feature = "performance")]
const EPD_COMMAND_POWER_ON: u8 = 0x04;

#[cfg(feature = "performance")]
const EPD_COMMAND_DISPLAY_REFRESH: u8 = 0x12;

#[cfg(feature = "trace")]
const DISPLAY_ASYNC_TRACE_ID: u32 = 1;

#[cfg(feature = "performance")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DisplayController {
    Ssd1677,
    Uc8179,
    Uc8279,
}

#[cfg(feature = "performance")]
#[derive(Debug, Clone, Copy)]
struct DisplayTracePlan {
    controller: DisplayController,
    presentation: PresentationMode,
    refresh: RefreshRequest,
    refresh_index: u8,
}

#[cfg(feature = "performance")]
impl DisplayTracePlan {
    const fn new(controller: DisplayController, update: FrameUpdate) -> Self {
        Self {
            controller,
            presentation: update.presentation(),
            refresh: update.refresh(),
            refresh_index: 0,
        }
    }

    fn next_refresh_phase(&mut self) -> DisplayPhase {
        let index = self.refresh_index;

        self.refresh_index = self.refresh_index.saturating_add(1);

        match (self.controller, self.presentation, self.refresh, index) {
            (_, PresentationMode::Binary, RefreshRequest::Full, _) => {
                DisplayPhase::BinaryFullRefresh
            }

            (_, PresentationMode::Binary, RefreshRequest::Fast, _) => {
                DisplayPhase::BinaryFastRefresh
            }

            (
                DisplayController::Uc8179,
                PresentationMode::Gray4 | PresentationMode::BinaryPreservingGray,
                RefreshRequest::Full,
                0,
            ) => DisplayPhase::GrayscaleBaseRefresh,

            (
                DisplayController::Uc8179,
                PresentationMode::Gray4 | PresentationMode::BinaryPreservingGray,
                RefreshRequest::Full,
                _,
            ) => DisplayPhase::GrayscaleActivate,

            (
                DisplayController::Uc8179,
                PresentationMode::Gray4 | PresentationMode::BinaryPreservingGray,
                RefreshRequest::Fast,
                0,
            ) => DisplayPhase::GrayscalePrecondition,

            (
                DisplayController::Uc8179,
                PresentationMode::Gray4 | PresentationMode::BinaryPreservingGray,
                RefreshRequest::Fast,
                _,
            ) => DisplayPhase::GrayscaleActivate,

            _ => DisplayPhase::GrayscaleRefresh,
        }
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
        "perf/present frame={=u32} cycles={=u32} io={=u64} busy={=u64} other={=u64} bytes={=u64} io_calls={=u32} busy_waits={=u32} longest_busy={=u32} busy_dropped={=u32}",
        frame_id,
        timings.total_cycles(),
        timings.io_cycles(),
        timings.busy_cycles(),
        timings.other_cycles(),
        timings.io_bytes(),
        timings.io_calls(),
        timings.busy_waits(),
        timings.longest_busy_cycles(),
        timings.dropped_busy_waits(),
    );

    info!(
        "perf/present_stream frame={=u32} cycles={=u64} bytes={=u64} calls={=u32}",
        frame_id,
        timings.stream_cycles(),
        timings.stream_bytes(),
        timings.stream_calls(),
    );

    for index in 0..timings.recorded_busy_waits() {
        let Some(cycles) = timings.busy_wait_cycles(index) else {
            continue;
        };

        info!(
            "perf/present_busy frame={=u32} index={=u8} cycles={=u32}",
            frame_id, index, cycles,
        );
    }
}

#[cfg(feature = "performance")]
pub(crate) fn log_frame(update: FrameUpdate, present: PresentTimings) {
    let report = update.perf_report();
    let timings = report.render();
    let damage = update.physical_damage();
    let eink = update.eink_report();

    info!(
        "perf/frame id={=u32} refresh={:?} presentation={:?} x={=u16} y={=u16} width={=u16} height={=u16} rebuild={=u32} layout={=u32} clear={=u32} paint={=u32} damage={=u32} present={=u32} present_io={=u64} present_busy={=u64} present_other={=u64} present_bytes={=u64} present_io_calls={=u32} present_busy_waits={=u32} present_longest_busy={=u32} present_busy_dropped={=u32} framebuffer_pixels={=u64} framebuffer_valid={=u8} text_draws={=u64} glyphs={=u64}",
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
        present.dropped_busy_waits(),
        report.framebuffer_pixels(),
        u8::from(report.has_framebuffer_pixels()),
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

    defmt::info!(
        "perf/pair_cache frame={=u32} lookups={=u64} hits={=u64} misses={=u64} collisions={=u64}",
        frame_id,
        paint.pair_positioning_cache_lookups(),
        paint.pair_positioning_cache_hits(),
        paint.pair_positioning_cache_misses(),
        paint.pair_positioning_cache_collisions(),
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
        "trace/session id={=u32} frame={=u32} hz={=u32} origin={=u32} spans={=u32} dropped={=u32} open={=u8}",
        summary.session_id(),
        frame_id,
        CLOCK_HZ,
        summary.origin_cycles(),
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

    log_trace_metrics(frame_id);
    log_reader_pagination_aggregates(frame_id);

    let async_records = inkpaper_trace::async_record_count();

    info!(
        "trace/async_summary records={=usize} dropped={=u32} open={=u8}",
        async_records,
        inkpaper_trace::async_dropped(),
        inkpaper_trace::async_open_count(),
    );

    for index in 0..async_records {
        let Some(record) = inkpaper_trace::async_record(index) else {
            continue;
        };

        info!(
            "trace/async event={=u8} id={=u32} start={=u32} cycles={=u32} arg={=u32}",
            record.event().id(),
            record.id(),
            record.start_cycles(),
            record.duration_cycles(),
            record.arg(),
        );
    }

    inkpaper_trace::clear_async_records();
}

#[cfg(feature = "performance")]
pub(crate) fn log_render_invalidation(frame_id: u32, invalidation: inkpaper_ui::Invalidation) {
    info!(
        "perf/invalidation frame={=u32} kind={:?}",
        frame_id, invalidation,
    );
}

#[cfg(feature = "trace")]
fn log_trace_metrics(frame_id: u32) {
    use inkpaper_trace::{TraceMetric, metric_record};

    let text_measure = metric_record(TraceMetric::TextMeasure);

    let bidi_build = metric_record(TraceMetric::BidiBuildRuns);
    let bidi_levels = metric_record(TraceMetric::BidiResolveLevels);
    let bidi_mirror = metric_record(TraceMetric::BidiMirror);
    let bidi_reorder = metric_record(TraceMetric::BidiReorder);

    let font_resolve = metric_record(TraceMetric::FontResolve);
    let pair = metric_record(TraceMetric::PairPositioning);
    let face_parse = metric_record(TraceMetric::TtfFaceParse);
    let gpos = metric_record(TraceMetric::GposPairLookup);
    let legacy = metric_record(TraceMetric::LegacyKerning);
    let mark_anchors = metric_record(TraceMetric::MarkAnchors);
    let mark_metrics = metric_record(TraceMetric::MarkMetrics);

    if text_measure.calls() != 0
        || bidi_build.calls() != 0
        || pair.calls() != 0
        || face_parse.calls() != 0
    {
        info!(
            "perf/text_shape frame={=u32} measure_calls={=u32} measure_cycles={=u64} bidi_build_calls={=u32} bidi_build_cycles={=u64} bidi_levels_cycles={=u64} bidi_mirror_cycles={=u64} bidi_reorder_cycles={=u64}",
            frame_id,
            text_measure.calls(),
            text_measure.cycles(),
            bidi_build.calls(),
            bidi_build.cycles(),
            bidi_levels.cycles(),
            bidi_mirror.cycles(),
            bidi_reorder.cycles(),
        );

        info!(
            "perf/text_position frame={=u32} resolve_calls={=u32} resolve_cycles={=u64} pair_calls={=u32} pair_cycles={=u64} face_parse_calls={=u32} face_parse_cycles={=u64} gpos_calls={=u32} gpos_cycles={=u64} legacy_calls={=u32} legacy_cycles={=u64} mark_anchor_calls={=u32} mark_anchor_cycles={=u64} mark_metric_calls={=u32} mark_metric_cycles={=u64}",
            frame_id,
            font_resolve.calls(),
            font_resolve.cycles(),
            pair.calls(),
            pair.cycles(),
            face_parse.calls(),
            face_parse.cycles(),
            gpos.calls(),
            gpos.cycles(),
            legacy.calls(),
            legacy.cycles(),
            mark_anchors.calls(),
            mark_anchors.cycles(),
            mark_metrics.calls(),
            mark_metrics.cycles(),
        );
    }
}

#[cfg(feature = "trace")]
fn log_reader_pagination_aggregates(frame_id: u32) {
    use inkpaper_trace::{TraceAggregate, aggregate_record, clear_aggregate_records};

    let measure = aggregate_record(TraceAggregate::ReaderMeasureText);
    let measure_bytes = aggregate_record(TraceAggregate::ReaderMeasureTextBytes);

    let resolve_font = aggregate_record(TraceAggregate::ReaderMeasureResolveFont);
    let shape = aggregate_record(TraceAggregate::ReaderMeasureShape);

    let boundary = aggregate_record(TraceAggregate::ReaderNextBoundary);
    let boundary_bytes = aggregate_record(TraceAggregate::ReaderNextBoundaryBytes);

    let line_height = aggregate_record(TraceAggregate::ReaderLineHeight);

    let blocks = aggregate_record(TraceAggregate::ReaderPaginationBlocks);
    let chars = aggregate_record(TraceAggregate::ReaderPaginationContentChars);
    let words = aggregate_record(TraceAggregate::ReaderPaginationWords);
    let whitespace = aggregate_record(TraceAggregate::ReaderPaginationWhitespaceRuns);
    let oversized = aggregate_record(TraceAggregate::ReaderPaginationOversizedWords);
    let fragments = aggregate_record(TraceAggregate::ReaderPaginationTextFragments);
    let lines = aggregate_record(TraceAggregate::ReaderPaginationLines);
    let pages = aggregate_record(TraceAggregate::ReaderPaginationPages);
    let images = aggregate_record(TraceAggregate::ReaderPaginationImages);

    let text_runs = aggregate_record(TraceAggregate::ReaderPaginationTextRuns);
    let text_run_bytes = aggregate_record(TraceAggregate::ReaderPaginationTextRunBytes);

    if measure.calls() != 0
        || boundary.calls() != 0
        || line_height.calls() != 0
        || blocks.calls() != 0
    {
        info!(
            "perf/reader_paginate frame={=u32} measure_calls={=u32} measure_cycles={=u64} measure_max_cycles={=u64} measure_bytes={=u64} measure_max_bytes={=u64} boundary_calls={=u32} boundary_cycles={=u64} boundary_max_cycles={=u64} boundary_bytes={=u64} boundary_max_bytes={=u64} line_height_calls={=u32} line_height_cycles={=u64} line_height_max_cycles={=u64} resolve_calls={=u32} resolve_cycles={=u64} resolve_max_cycles={=u64} shape_calls={=u32} shape_cycles={=u64} shape_max_cycles={=u64}",
            frame_id,
            measure.calls(),
            measure.total(),
            measure.max(),
            measure_bytes.total(),
            measure_bytes.max(),
            boundary.calls(),
            boundary.total(),
            boundary.max(),
            boundary_bytes.total(),
            boundary_bytes.max(),
            line_height.calls(),
            line_height.total(),
            line_height.max(),
            resolve_font.calls(),
            resolve_font.total(),
            resolve_font.max(),
            shape.calls(),
            shape.total(),
            shape.max(),
        );

        info!(
            "perf/reader_pagination_work frame={=u32} blocks={=u64} chars={=u64} words={=u64} whitespace_runs={=u64} oversized_words={=u64} fragments={=u64} lines={=u64} pages={=u64} images={=u64} text_runs={=u64} text_run_bytes={=u64} text_run_max_bytes={=u64}",
            frame_id,
            blocks.total(),
            chars.total(),
            words.total(),
            whitespace.total(),
            oversized.total(),
            fragments.total(),
            lines.total(),
            pages.total(),
            images.total(),
            text_runs.total(),
            text_run_bytes.total(),
            text_run_bytes.max(),
        );
    }

    clear_aggregate_records();
}
