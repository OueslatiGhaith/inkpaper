use core::fmt::Write;

use defmt::info;
use embedded_hal_async::delay::DelayNs;
use epd_bus::{BusyPolarity, DisplayPhase, EpdInterface};
use esp_hal::time::Instant;
use esp_hal::xtensa_lx::timer::get_cycle_count;

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
}

impl<'a, B> ProfiledEpdBus<'a, B> {
    pub(crate) fn new(inner: &'a mut B) -> Self {
        Self {
            inner,
            metrics: PresentMetrics::default(),
            next_busy_wait_index: 0,
        }
    }

    pub(crate) fn finish(self) -> PresentMetrics {
        self.metrics
    }

    #[inline(always)]
    fn record_io(&mut self, cycles: u32, bytes: u64) {
        self.metrics.record_io(cycles, bytes);
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

        result
    }

    async fn data(&mut self, data: &[u8]) -> Result<(), Self::Error> {
        let timer = CycleTimer::start();

        let result = self.inner.data(data).await;

        self.record_io(timer.elapsed(), usize_to_u64(data.len()));

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

    fn reset_high(&mut self) -> Result<(), Self::Error> {
        self.inner.reset_high()
    }

    fn is_busy(&mut self, polarity: BusyPolarity) -> Result<bool, Self::Error> {
        self.inner.is_busy(polarity)
    }

    async fn wait_busy<D>(
        &mut self,
        polarity: BusyPolarity,
        delay: &mut D,
    ) -> Result<(), Self::Error>
    where
        D: DelayNs,
    {
        self.wait_busy_for(polarity, DisplayPhase::Unknown, delay)
            .await
    }

    async fn wait_busy_for<D>(
        &mut self,
        polarity: BusyPolarity,
        phase: DisplayPhase,
        delay: &mut D,
    ) -> Result<(), Self::Error>
    where
        D: DelayNs,
    {
        let wait_index = self.next_busy_wait_index;

        self.next_busy_wait_index = self.next_busy_wait_index.saturating_add(1);

        let phase_trace = trace_display_phase(phase);

        let busy_trace = inkpaper_trace::async_span!(
            target: "display.present",
            "busy_wait",
            index = wait_index,
        );

        let result = self.inner.wait_busy_for(polarity, phase, delay).await;

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

#[inline(always)]
fn trace_display_phase(phase: DisplayPhase) -> inkpaper_trace::AsyncSpan {
    match phase {
        DisplayPhase::Unknown => {
            inkpaper_trace::async_span!(target: "display.phase", "unknown")
        }

        DisplayPhase::PowerOn => {
            inkpaper_trace::async_span!(target: "display.phase", "power_on")
        }

        DisplayPhase::BinaryFullRefresh => {
            inkpaper_trace::async_span!(target: "display.phase", "binary_full_refresh")
        }

        DisplayPhase::BinaryFastRefresh => {
            inkpaper_trace::async_span!(target: "display.phase", "binary_fast_refresh")
        }

        DisplayPhase::GrayscaleBaseRefresh => {
            inkpaper_trace::async_span!(target: "display.phase", "grayscale_base_refresh")
        }

        DisplayPhase::GrayscalePrecondition => {
            inkpaper_trace::async_span!(target: "display.phase", "grayscale_precondition")
        }

        DisplayPhase::GrayscaleActivate => {
            inkpaper_trace::async_span!(target: "display.phase", "grayscale_activate")
        }

        DisplayPhase::GrayscaleRefresh => {
            inkpaper_trace::async_span!(target: "display.phase", "grayscale_refresh")
        }

        DisplayPhase::PowerOff => {
            inkpaper_trace::async_span!(target: "display.phase", "power_off")
        }
    }
}

fn usize_to_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

macro_rules! record_gauges {
    ($target:literal => { $($name:literal = $value:expr $(=> $unit:literal)?),* $(,)? }) => {
        $(
            record_gauges!(@emit $target, $name, $value $(, $unit)?);
        )*
    };
    (@emit $target:literal, $name:literal, $value:expr) => {
        inkpaper_trace::gauge!(target: $target, $name, $value);
    };
    (@emit $target:literal, $name:literal, $value:expr, $unit:literal) => {
        inkpaper_trace::gauge!(target: $target, $name, $value, unit: $unit);
    };
}

pub(crate) fn record_present_metrics(metrics: PresentMetrics) {
    record_gauges!("display.present" => {
        "io_cycles" = metrics.io_cycles() => "cycles",
        "bytes" = metrics.io_bytes() => "bytes",
        "io_calls" = metrics.io_calls(),
    });

    record_gauges!("display.stream" => {
        "cycles" = metrics.stream_cycles() => "cycles",
        "bytes" = metrics.stream_bytes() => "bytes",
        "calls" = metrics.stream_calls(),
    });
}

pub(crate) fn record_ui_metrics(metrics: inkpaper_ui::PerformanceMetrics) {
    record_gauges!("ui.render" => {
        "entity_render_calls" = metrics.entity_render_calls,
        "nodes_mounted" = metrics.nodes_mounted,
    });

    record_gauges!("ui.layout" => {
        "measure_node_calls" = metrics.measure_node_calls,
        "measurement_cache_hits" = metrics.measurement_cache_hits,
        "measurement_cache_misses" = metrics.measurement_cache_misses,
        "text_measurements" = metrics.text_measurements,
        "flex_base_main_size_calls" = metrics.flex_base_main_size_calls,
        "flex_item_main_size_calls" = metrics.flex_item_main_size_calls,
        "flex_sibling_visits" = metrics.flex_sibling_visits,
        "nodes_laid_out" = metrics.nodes_laid_out,
    });

    record_gauges!("ui.visual" => {
        "visual_traversal_passes" = metrics.visual_traversal_passes,
        "visual_traversal_nodes" = metrics.visual_traversal_nodes,
        "visual_context_derivations" = metrics.visual_context_derivations,
        "visual_clip_intersections" = metrics.visual_clip_intersections,
        "visual_ancestor_pushes" = metrics.visual_ancestor_pushes,
        "visual_ancestor_pops" = metrics.visual_ancestor_pops,
        "visual_ancestor_depth_peak" = metrics.visual_ancestor_depth_peak,
        "visual_context_stack_pushes" = metrics.visual_context_stack_pushes,
        "visual_context_stack_pops" = metrics.visual_context_stack_pops,
        "visual_context_stack_depth_peak" = metrics.visual_context_stack_depth_peak,
        "visual_context_stack_overflows" = metrics.visual_context_stack_overflows,
        "visual_context_recomputations" = metrics.visual_context_recomputations,
        "visual_bounds_ancestor_visits" = metrics.visual_bounds_ancestor_visits,
        "scroll_into_view_ancestor_visits" = metrics.scroll_into_view_ancestor_visits,
        "visual_nodes_visited" = metrics.visual_nodes_visited,
        "visible_nodes" = metrics.visible_nodes,
    });

    record_gauges!("ui.damage" => {
        "render_invalidations_consumed" = metrics.render_invalidations_consumed,
        "full_damage_invalidations" = metrics.full_damage_invalidations,
        "partial_damage_invalidations" = metrics.partial_damage_invalidations,
        "damage_rectangles" = metrics.damage_rectangles,
        "damage_culled_nodes" = metrics.damage_culled_nodes,
        "damage_pruned_subtrees" = metrics.damage_pruned_subtrees,
        "damage_extent_pruned_subtrees" = metrics.damage_extent_pruned_subtrees,
        "damage_pruned_sibling_runs" = metrics.damage_pruned_sibling_runs,
        "damage_pruned_sibling_prefixes" = metrics.damage_pruned_sibling_prefixes,
        "damage_prefix_search_steps" = metrics.damage_prefix_search_steps,
        "damage_clip_paints" = metrics.damage_clip_paints,
    });

    record_gauges!("ui.paint" => {
        "nodes_painted" = metrics.nodes_painted,
    });
}

pub(crate) fn record_text_metrics(
    text: inkpaper_ui::TextPaintMetrics,
    cache: inkpaper_ui::GlyphCacheMetrics,
    bytes_used: usize,
    bytes_capacity: usize,
) {
    record_gauges!("ui.text" => {
        "draw_calls" = text.draw_calls,
        "shaped_glyphs" = text.shaped_glyphs,
    });

    record_gauges!("ui.glyph_cache" => {
        "lookups" = cache.lookups,
        "hits" = cache.hits,
        "misses" = cache.misses,
        "collisions" = cache.collisions,
        "rasterizations" = cache.rasterizations,
        "clears" = cache.clears,
        "bytes_used" = bytes_used => "bytes",
        "bytes_peak" = cache.bytes_peak => "bytes",
        "bytes_capacity" = bytes_capacity => "bytes",
    });

    record_gauges!("ui.pair_cache" => {
        "lookups" = text.pair_cache_lookups,
        "hits" = text.pair_cache_hits,
        "misses" = text.pair_cache_misses,
        "collisions" = text.pair_cache_collisions,
    });
}

pub(crate) fn record_coverage_metrics(
    paint: inkpaper_ui_eink::EInkPaintReport,
    framebuffer_draw_iter_pixels: u64,
) {
    record_gauges!("ui.coverage" => {
        "bitmaps" = paint.coverage_bitmaps(),
        "samples" = paint.coverage_samples() => "pixels",
        "accepted" = paint.coverage_accepted() => "pixels",
        "framebuffer_pixels" = framebuffer_draw_iter_pixels => "pixels",
    });
}

pub(crate) fn record_coverage_blitter_metrics(calls: u64, pixels: u64, cycles: u64) {
    record_gauges!("ui.coverage_fast" => {
        "calls" = calls,
        "pixels" = pixels => "pixels",
        "cycles" = cycles => "cycles",
    });
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
