use core::fmt::Write;

use defmt::info;
use embedded_hal_async::delay::DelayNs;
use epd_bus::{BusyPolarity, EpdInterface};
use esp_hal::time::Instant;
use esp_hal::xtensa_lx::timer::get_cycle_count;

use crate::firmware::{
    presenter::{FrameUpdate, PresentationMode},
    refresh_policy::RefreshRequest,
};

pub(crate) const CLOCK_HZ: u32 = 240_000_000;
pub(crate) const TRACE_MONOTONIC_HZ: u32 = 1_000_000;

pub(crate) fn trace_monotonic_ticks() -> u64 {
    Instant::now().duration_since_epoch().as_micros()
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct CycleTimer {
    started_at: u32,
}

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

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PresentMetrics {
    io_cycles: u64,
    io_bytes: u64,
    io_calls: u32,

    stream_cycles: u64,
    stream_bytes: u64,
    stream_calls: u32,
}

impl PresentMetrics {
    pub(crate) const fn io_cycles(self) -> u64 {
        self.io_cycles
    }

    pub(crate) const fn io_bytes(self) -> u64 {
        self.io_bytes
    }

    pub(crate) const fn io_calls(self) -> u32 {
        self.io_calls
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

    fn record_io(&mut self, cycles: u32, bytes: u64) {
        self.io_cycles = self.io_cycles.saturating_add(u64::from(cycles));
        self.io_bytes = self.io_bytes.saturating_add(bytes);
        self.io_calls = self.io_calls.saturating_add(1);
    }

    fn record_stream_io(&mut self, cycles: u32, bytes: u64) {
        self.record_io(cycles, bytes);

        self.stream_cycles = self.stream_cycles.saturating_add(u64::from(cycles));
        self.stream_bytes = self.stream_bytes.saturating_add(bytes);
        self.stream_calls = self.stream_calls.saturating_add(1);
    }
}

pub(crate) struct ProfiledEpdBus<'a, B> {
    inner: &'a mut B,
    metrics: PresentMetrics,

    next_busy_wait_index: u32,

    trace_plan: Option<DisplayTracePlan>,
    pending_phase: Option<DisplayPhase>,
}

impl<'a, B> ProfiledEpdBus<'a, B> {
    pub(crate) fn new(
        inner: &'a mut B,
        controller: DisplayController,
        update: FrameUpdate,
    ) -> Self {
        Self {
            inner,
            metrics: PresentMetrics::default(),

            next_busy_wait_index: 0,

            trace_plan: Some(DisplayTracePlan::new(controller, update)),
            pending_phase: None,
        }
    }

    pub(crate) fn new_preparation(inner: &'a mut B) -> Self {
        Self {
            inner,
            metrics: PresentMetrics::default(),

            next_busy_wait_index: 0,

            trace_plan: None,
            pending_phase: None,
        }
    }

    pub(crate) fn finish(self) -> PresentMetrics {
        self.metrics
    }

    #[inline(always)]
    fn record_io(&mut self, cycles: u32, bytes: u64) {
        self.metrics.record_io(cycles, bytes);
    }

    fn observe_command(&mut self, command: u8) {
        let phase = match command {
            EPD_COMMAND_POWER_ON => Some(DisplayPhase::PowerOn),

            EPD_COMMAND_DISPLAY_REFRESH => {
                let phase = self
                    .trace_plan
                    .as_mut()
                    .map(DisplayTracePlan::next_refresh_phase)
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
        let result = self.inner.is_busy(polarity);

        let ready = match &result {
            Ok(busy) => !*busy,
            Err(_) => false,
        };

        if ready && self.pending_phase == Some(DisplayPhase::PowerOff) {
            self.pending_phase = None;
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
        let wait_index = self.next_busy_wait_index;

        self.next_busy_wait_index = self.next_busy_wait_index.saturating_add(1);

        let phase = self.pending_phase.take().unwrap_or(DisplayPhase::Unknown);

        let phase_trace = phase.trace_async_span();

        let busy_trace = inkpaper_trace::async_span!(
            target: "display.present",
            "busy_wait",
            index = wait_index,
        );

        let result = self.inner.wait_busy(polarity, delay).await;

        drop(busy_trace);
        drop(phase_trace);

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

        self.metrics
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

const EPD_COMMAND_POWER_OFF: u8 = 0x02;

const EPD_COMMAND_POWER_ON: u8 = 0x04;

const EPD_COMMAND_DISPLAY_REFRESH: u8 = 0x12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DisplayController {
    Ssd1677,
    Uc8179,
    Uc8279,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DisplayPhase {
    Unknown,
    PowerOn,
    BinaryFullRefresh,
    BinaryFastRefresh,
    GrayscaleBaseRefresh,
    GrayscalePrecondition,
    GrayscaleActivate,
    GrayscaleRefresh,
    PowerOff,
}

impl DisplayPhase {
    #[inline(always)]
    fn trace_async_span(self) -> inkpaper_trace::AsyncSpan {
        match self {
            Self::Unknown => {
                inkpaper_trace::async_span!(target: "display.phase", "unknown")
            }

            Self::PowerOn => {
                inkpaper_trace::async_span!(target: "display.phase", "power_on")
            }

            Self::BinaryFullRefresh => {
                inkpaper_trace::async_span!(target: "display.phase", "binary_full_refresh")
            }

            Self::BinaryFastRefresh => {
                inkpaper_trace::async_span!(target: "display.phase", "binary_fast_refresh")
            }

            Self::GrayscaleBaseRefresh => {
                inkpaper_trace::async_span!(target: "display.phase", "grayscale_base_refresh")
            }

            Self::GrayscalePrecondition => {
                inkpaper_trace::async_span!(target: "display.phase", "grayscale_precondition")
            }

            Self::GrayscaleActivate => {
                inkpaper_trace::async_span!(target: "display.phase", "grayscale_activate")
            }

            Self::GrayscaleRefresh => {
                inkpaper_trace::async_span!(target: "display.phase", "grayscale_refresh")
            }

            Self::PowerOff => {
                inkpaper_trace::async_span!(target: "display.phase", "power_off")
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct DisplayTracePlan {
    controller: DisplayController,
    presentation: PresentationMode,
    refresh: RefreshRequest,
    refresh_index: u8,
}

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

fn usize_to_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RenderTimings {
    pub(crate) rebuild_cycles: u32,
    pub(crate) layout_cycles: u32,
    pub(crate) clear_cycles: u32,
    pub(crate) paint_cycles: u32,
    pub(crate) damage_cycles: u32,
}

pub(crate) fn record_render_metrics(timings: RenderTimings) {
    inkpaper_trace::gauge!(
        target: "ui.render",
        "rebuild_cycles",
        timings.rebuild_cycles,
        unit: "cycles",
    );

    inkpaper_trace::gauge!(
        target: "ui.render",
        "layout_cycles",
        timings.layout_cycles,
        unit: "cycles",
    );

    inkpaper_trace::gauge!(
        target: "ui.render",
        "clear_cycles",
        timings.clear_cycles,
        unit: "cycles",
    );

    inkpaper_trace::gauge!(
        target: "ui.render",
        "paint_cycles",
        timings.paint_cycles,
        unit: "cycles",
    );

    inkpaper_trace::gauge!(
        target: "ui.render",
        "damage_cycles",
        timings.damage_cycles,
        unit: "cycles",
    );
}

pub(crate) fn record_present_metrics(metrics: PresentMetrics) {
    inkpaper_trace::gauge!(
        target: "display.present",
        "io_cycles",
        metrics.io_cycles(),
        unit: "cycles",
    );

    inkpaper_trace::gauge!(
        target: "display.present",
        "bytes",
        metrics.io_bytes(),
        unit: "bytes",
    );

    inkpaper_trace::gauge!(
        target: "display.present",
        "io_calls",
        metrics.io_calls(),
    );

    inkpaper_trace::gauge!(
        target: "display.stream",
        "cycles",
        metrics.stream_cycles(),
        unit: "cycles",
    );

    inkpaper_trace::gauge!(
        target: "display.stream",
        "bytes",
        metrics.stream_bytes(),
        unit: "bytes",
    );

    inkpaper_trace::gauge!(
        target: "display.stream",
        "calls",
        metrics.stream_calls(),
    );
}

pub(crate) fn record_ui_metrics(metrics: inkpaper_ui::PerformanceMetrics) {
    inkpaper_trace::gauge!(
        target: "ui.render",
        "entity_render_calls",
        metrics.entity_render_calls,
    );

    inkpaper_trace::gauge!(
        target: "ui.render",
        "nodes_mounted",
        metrics.nodes_mounted,
    );

    inkpaper_trace::gauge!(
        target: "ui.layout",
        "measure_node_calls",
        metrics.measure_node_calls,
    );

    inkpaper_trace::gauge!(
        target: "ui.layout",
        "measurement_cache_hits",
        metrics.measurement_cache_hits,
    );

    inkpaper_trace::gauge!(
        target: "ui.layout",
        "measurement_cache_misses",
        metrics.measurement_cache_misses,
    );

    inkpaper_trace::gauge!(
        target: "ui.layout",
        "text_measurements",
        metrics.text_measurements,
    );

    inkpaper_trace::gauge!(
        target: "ui.layout",
        "flex_base_main_size_calls",
        metrics.flex_base_main_size_calls,
    );

    inkpaper_trace::gauge!(
        target: "ui.layout",
        "flex_item_main_size_calls",
        metrics.flex_item_main_size_calls,
    );

    inkpaper_trace::gauge!(
        target: "ui.layout",
        "flex_sibling_visits",
        metrics.flex_sibling_visits,
    );

    inkpaper_trace::gauge!(
        target: "ui.layout",
        "nodes_laid_out",
        metrics.nodes_laid_out,
    );

    inkpaper_trace::gauge!(
        target: "ui.visual",
        "visual_traversal_passes",
        metrics.visual_traversal_passes,
    );

    inkpaper_trace::gauge!(
        target: "ui.visual",
        "visual_traversal_nodes",
        metrics.visual_traversal_nodes,
    );

    inkpaper_trace::gauge!(
        target: "ui.visual",
        "visual_context_derivations",
        metrics.visual_context_derivations,
    );

    inkpaper_trace::gauge!(
        target: "ui.visual",
        "visual_clip_intersections",
        metrics.visual_clip_intersections,
    );

    inkpaper_trace::gauge!(
        target: "ui.visual",
        "visual_ancestor_pushes",
        metrics.visual_ancestor_pushes,
    );

    inkpaper_trace::gauge!(
        target: "ui.visual",
        "visual_ancestor_pops",
        metrics.visual_ancestor_pops,
    );

    inkpaper_trace::gauge!(
        target: "ui.visual",
        "visual_ancestor_depth_peak",
        metrics.visual_ancestor_depth_peak,
    );

    inkpaper_trace::gauge!(
        target: "ui.visual",
        "visual_context_stack_pushes",
        metrics.visual_context_stack_pushes,
    );

    inkpaper_trace::gauge!(
        target: "ui.visual",
        "visual_context_stack_pops",
        metrics.visual_context_stack_pops,
    );

    inkpaper_trace::gauge!(
        target: "ui.visual",
        "visual_context_stack_depth_peak",
        metrics.visual_context_stack_depth_peak,
    );

    inkpaper_trace::gauge!(
        target: "ui.visual",
        "visual_context_stack_overflows",
        metrics.visual_context_stack_overflows,
    );

    inkpaper_trace::gauge!(
        target: "ui.visual",
        "visual_context_recomputations",
        metrics.visual_context_recomputations,
    );

    inkpaper_trace::gauge!(
        target: "ui.visual",
        "visual_bounds_ancestor_visits",
        metrics.visual_bounds_ancestor_visits,
    );

    inkpaper_trace::gauge!(
        target: "ui.visual",
        "scroll_into_view_ancestor_visits",
        metrics.scroll_into_view_ancestor_visits,
    );

    inkpaper_trace::gauge!(
        target: "ui.damage",
        "render_invalidations_consumed",
        metrics.render_invalidations_consumed,
    );

    inkpaper_trace::gauge!(
        target: "ui.damage",
        "full_damage_invalidations",
        metrics.full_damage_invalidations,
    );

    inkpaper_trace::gauge!(
        target: "ui.damage",
        "partial_damage_invalidations",
        metrics.partial_damage_invalidations,
    );

    inkpaper_trace::gauge!(
        target: "ui.damage",
        "damage_rectangles",
        metrics.damage_rectangles,
    );

    inkpaper_trace::gauge!(
        target: "ui.damage",
        "damage_culled_nodes",
        metrics.damage_culled_nodes,
    );

    inkpaper_trace::gauge!(
        target: "ui.damage",
        "damage_pruned_subtrees",
        metrics.damage_pruned_subtrees,
    );

    inkpaper_trace::gauge!(
        target: "ui.damage",
        "damage_extent_pruned_subtrees",
        metrics.damage_extent_pruned_subtrees,
    );

    inkpaper_trace::gauge!(
        target: "ui.damage",
        "damage_pruned_sibling_runs",
        metrics.damage_pruned_sibling_runs,
    );

    inkpaper_trace::gauge!(
        target: "ui.damage",
        "damage_pruned_sibling_prefixes",
        metrics.damage_pruned_sibling_prefixes,
    );

    inkpaper_trace::gauge!(
        target: "ui.damage",
        "damage_prefix_search_steps",
        metrics.damage_prefix_search_steps,
    );

    inkpaper_trace::gauge!(
        target: "ui.damage",
        "damage_clip_paints",
        metrics.damage_clip_paints,
    );

    inkpaper_trace::gauge!(
        target: "ui.visual",
        "visual_nodes_visited",
        metrics.visual_nodes_visited,
    );

    inkpaper_trace::gauge!(
        target: "ui.visual",
        "visible_nodes",
        metrics.visible_nodes,
    );

    inkpaper_trace::gauge!(
        target: "ui.paint",
        "nodes_painted",
        metrics.nodes_painted,
    );
}

pub(crate) fn record_text_metrics(
    paint: inkpaper_ui::backend::EInkPaintReport,
    cache: inkpaper_ui::GlyphCacheMetrics,
    bytes_used: usize,
    bytes_capacity: usize,
) {
    inkpaper_trace::gauge!(
        target: "ui.text",
        "draw_calls",
        paint.text_draw_calls(),
    );

    inkpaper_trace::gauge!(
        target: "ui.text",
        "shaped_glyphs",
        paint.shaped_glyphs(),
    );

    inkpaper_trace::gauge!(
        target: "ui.glyph_cache",
        "lookups",
        cache.lookups,
    );

    inkpaper_trace::gauge!(
        target: "ui.glyph_cache",
        "hits",
        cache.hits,
    );

    inkpaper_trace::gauge!(
        target: "ui.glyph_cache",
        "misses",
        cache.misses,
    );

    inkpaper_trace::gauge!(
        target: "ui.glyph_cache",
        "collisions",
        cache.collisions,
    );

    inkpaper_trace::gauge!(
        target: "ui.glyph_cache",
        "rasterizations",
        cache.rasterizations,
    );

    inkpaper_trace::gauge!(
        target: "ui.glyph_cache",
        "clears",
        cache.clears,
    );

    inkpaper_trace::gauge!(
        target: "ui.glyph_cache",
        "bytes_used",
        bytes_used,
        unit: "bytes",
    );

    inkpaper_trace::gauge!(
        target: "ui.glyph_cache",
        "bytes_peak",
        cache.bytes_peak,
        unit: "bytes",
    );

    inkpaper_trace::gauge!(
        target: "ui.glyph_cache",
        "bytes_capacity",
        bytes_capacity,
        unit: "bytes",
    );

    inkpaper_trace::gauge!(
        target: "ui.pair_cache",
        "lookups",
        paint.pair_positioning_cache_lookups(),
    );

    inkpaper_trace::gauge!(
        target: "ui.pair_cache",
        "hits",
        paint.pair_positioning_cache_hits(),
    );

    inkpaper_trace::gauge!(
        target: "ui.pair_cache",
        "misses",
        paint.pair_positioning_cache_misses(),
    );

    inkpaper_trace::gauge!(
        target: "ui.pair_cache",
        "collisions",
        paint.pair_positioning_cache_collisions(),
    );
}

pub(crate) fn record_coverage_metrics(
    paint: inkpaper_ui::backend::EInkPaintReport,
    framebuffer_draw_iter_pixels: u64,
) {
    inkpaper_trace::gauge!(
        target: "ui.coverage",
        "bitmaps",
        paint.coverage_bitmaps(),
    );

    inkpaper_trace::gauge!(
        target: "ui.coverage",
        "samples",
        paint.coverage_samples(),
        unit: "pixels",
    );

    inkpaper_trace::gauge!(
        target: "ui.coverage",
        "accepted",
        paint.coverage_accepted(),
        unit: "pixels",
    );

    inkpaper_trace::gauge!(
        target: "ui.coverage",
        "framebuffer_pixels",
        framebuffer_draw_iter_pixels,
        unit: "pixels",
    );
}

pub(crate) fn record_ordered_coverage_metrics(calls: u64, pixels: u64, cycles: u64) {
    inkpaper_trace::gauge!(
        target: "ui.coverage_fast",
        "calls",
        calls,
    );

    inkpaper_trace::gauge!(
        target: "ui.coverage_fast",
        "pixels",
        pixels,
        unit: "pixels",
    );

    inkpaper_trace::gauge!(
        target: "ui.coverage_fast",
        "cycles",
        cycles,
        unit: "cycles",
    );
}

struct DefmtTraceWriter<const N: usize> {
    bytes: [u8; N],
    len: usize,
}

impl<const N: usize> DefmtTraceWriter<N> {
    const fn new() -> Self {
        Self {
            bytes: [0; N],
            len: 0,
        }
    }

    fn push(&mut self, value: &str) -> core::fmt::Result {
        let end = self.len.checked_add(value.len()).ok_or(core::fmt::Error)?;

        if end > self.bytes.len() {
            return Err(core::fmt::Error);
        }

        self.bytes[self.len..end].copy_from_slice(value.as_bytes());

        self.len = end;

        Ok(())
    }

    fn flush_line(&mut self) -> core::fmt::Result {
        if self.len == 0 {
            return Ok(());
        }

        let line = core::str::from_utf8(&self.bytes[..self.len]).map_err(|_| core::fmt::Error)?;

        defmt::info!("{=str}", line);

        self.len = 0;

        Ok(())
    }

    fn finish(&mut self) -> core::fmt::Result {
        self.flush_line()
    }
}

impl<const N: usize> Write for DefmtTraceWriter<N> {
    fn write_str(&mut self, value: &str) -> core::fmt::Result {
        let mut remaining = value;

        while let Some(newline) = remaining.find('\n') {
            let line = &remaining[..newline];

            self.push(line)?;
            self.flush_line()?;

            remaining = &remaining[newline + 1..];
        }

        self.push(remaining)
    }
}

pub(crate) fn log_trace(capture: inkpaper_trace::TraceCapture) {
    const TRACE_LINE_BYTES: usize = 512;

    let capture_id = capture.id();

    let mut writer = DefmtTraceWriter::<TRACE_LINE_BYTES>::new();

    let result = inkpaper_trace::write_text_capture(&capture, &mut writer).and_then(|_| {
        writer
            .finish()
            .map_err(|_| inkpaper_trace::TextEncodeError::Write)
    });

    if result.is_err() {
        info!("trace/v4 encode_error id={=u32}", capture_id,);
    }
}
